//! A file's thumbnail: drawn once, kept by the content it was drawn from. Every call first asks the
//! file's owner as the caller, so a removed grant ends it at once and a person without one learns
//! nothing from what is kept. What is kept is only small pictures, under names the content decides,
//! and any of it can be drawn again.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::{Duration, SystemTime},
};

use anyhow::Context as _;
use base64::Engine as _;
use sha2::{Digest as _, Sha256};
use tokio::{io::AsyncSeekExt as _, sync::Semaphore};
use veoveo_mcp_artifact::{
    client::ArtifactClient,
    contract::{ArtifactId, ReadArtifact},
};
use veoveo_mcp_usd::contract as usd;

use crate::{
    contract::{
        ArtifactRef, BranchName, FilePath, FileSource, RepositoryId, Thumbnail, ThumbnailRequest,
        ThumbnailSize,
    },
    hub::{Ends, HubError, Reader},
    render::{LARGE, Recipe, Renderer, SMALL, is_stage},
};

/// Changes when what is drawn changes, so that older pictures are drawn again.
const RECIPE: &str = "1";
/// How long a file that could not be drawn is left before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// How many files draw at once.
const CONCURRENT_DRAWS: usize = 3;
/// The media type of the pictures.
const MEDIA_TYPE: &str = "image/webp";
/// The most of a file that is read whole. A larger one that needs reading whole has no picture.
const MAX_WHOLE: u64 = 2 * 1024 * 1024 * 1024;
/// How much of a video's or a sound's start and end a frame is read from.
const HEAD: u64 = 32 * 1024 * 1024;
const TAIL: u64 = 16 * 1024 * 1024;

/// Why a picture was refused, as opposed to not drawn.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("{0}")]
    Hub(#[from] HubError),
    #[error("reading the artifact: {0}")]
    Artifact(String),
}

/// The kept pictures, the jobs and the draws in progress.
#[derive(Clone)]
pub struct Thumbnails {
    kept: PathBuf,
    work: PathBuf,
    renderer: Renderer,
    http: reqwest::Client,
    permits: Arc<Semaphore>,
    drawing: Arc<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
}

/// Where a file's bytes come from, as its caller.
enum Origin<'a> {
    Lore {
        reader: &'a Reader,
        repository: &'a RepositoryId,
        branch: &'a BranchName,
        path: &'a FilePath,
        size: u64,
    },
    Artifact {
        client: ArtifactClient,
        artifact: ArtifactId,
        size: u64,
    },
}

/// What is to be drawn.
struct Drawing<'a> {
    /// What names the file's content, apart from any place it is in.
    content: String,
    name: String,
    media_type: Option<String>,
    origin: Origin<'a>,
}

impl Thumbnails {
    /// Pictures are kept under `kept` and jobs run under `work`, which starts empty. Steps run
    /// through `program`'s `confine` subcommand.
    pub fn new(
        kept: PathBuf,
        work: PathBuf,
        program: PathBuf,
        http: reqwest::Client,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&kept).with_context(|| format!("making {}", kept.display()))?;
        match std::fs::remove_dir_all(&work) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error).with_context(|| format!("emptying {}", work.display()));
            }
            _ => {}
        }
        std::fs::create_dir_all(&work).with_context(|| format!("making {}", work.display()))?;
        Ok(Self {
            kept,
            work,
            renderer: Renderer::new(program),
            http,
            permits: Arc::new(Semaphore::new(CONCURRENT_DRAWS)),
            drawing: Arc::default(),
        })
    }

    /// The thumbnail of `request`'s file for its caller. A refusal of the file is an error; a file
    /// that cannot be drawn answers none.
    pub async fn thumbnail(
        &self,
        reader: &Reader,
        request: &ThumbnailRequest,
    ) -> Result<Thumbnail, ThumbnailError> {
        match &request.source {
            FileSource::Lore {
                repository,
                branch,
                path,
            } => {
                let probe = reader.probe(repository, branch, path).await?;
                let name = path.file_name().to_owned();
                if is_stage(&name) {
                    return Ok(scene(reader, repository, branch, path, request.size).await);
                }
                let drawing = Drawing {
                    content: format!("lore\0{}", probe.address),
                    name,
                    media_type: None,
                    origin: Origin::Lore {
                        reader,
                        repository,
                        branch,
                        path,
                        size: probe.size,
                    },
                };
                Ok(self.picture(drawing, request.size).await)
            }
            FileSource::Artifact { artifact } => {
                let id = artifact_id(artifact)?;
                let entry = reader
                    .client
                    .read(ReadArtifact { artifact: id })
                    .await
                    .map_err(|error| ThumbnailError::Artifact(error.to_string()))?;
                let record = entry.artifact;
                let drawing = Drawing {
                    content: format!("artifact\0{artifact}"),
                    name: record.name.to_string(),
                    media_type: Some(record.media_type.to_string()),
                    origin: Origin::Artifact {
                        client: ArtifactClient::new(reader.client.clone(), self.http.clone()),
                        artifact: id,
                        size: record.size.map(|size| size.bytes()).unwrap_or_default(),
                    },
                };
                Ok(self.picture(drawing, request.size).await)
            }
        }
    }

    /// The kept thumbnail, or a new one, or none.
    async fn picture(&self, drawing: Drawing<'_>, size: ThumbnailSize) -> Thumbnail {
        let none = Thumbnail::default();
        let Some(recipe) = Recipe::of(&drawing.name, drawing.media_type.as_deref()) else {
            return none;
        };
        let key = key(&drawing.content);
        if let Some(found) = self.kept_picture(&key, size).await {
            return found;
        }
        if self.failed_lately(&key).await {
            return none;
        }
        let turn = self.turn(&key);
        let _turn = turn.lock().await;
        if let Some(found) = self.kept_picture(&key, size).await {
            return found;
        }
        let Ok(_permit) = self.permits.acquire().await else {
            return none;
        };
        match self.draw(&drawing, recipe, &key).await {
            Ok(()) => self.kept_picture(&key, size).await.unwrap_or(none),
            Err(error) => {
                tracing::warn!(error = %format!("{error:#}"), ?recipe, "a picture could not be drawn");
                // A later call tries again once this is old. A failure to note it costs a retry.
                let _ = tokio::fs::write(self.failed_path(&key), b"").await;
                none
            }
        }
    }

    /// One draw of a key at a time, so that many asks for one file draw it once.
    fn turn(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut drawing = self.drawing.lock().unwrap_or_else(|held| held.into_inner());
        drawing.retain(|_, turn| turn.strong_count() > 0);
        if let Some(turn) = drawing.get(key).and_then(Weak::upgrade) {
            return turn;
        }
        let turn = Arc::new(tokio::sync::Mutex::new(()));
        drawing.insert(key.to_owned(), Arc::downgrade(&turn));
        turn
    }

    fn path(&self, key: &str, size: ThumbnailSize) -> PathBuf {
        self.kept.join(format!("{key}.{}", file_of(size)))
    }

    fn failed_path(&self, key: &str) -> PathBuf {
        self.kept.join(format!("{key}.failed"))
    }

    async fn kept_picture(&self, key: &str, size: ThumbnailSize) -> Option<Thumbnail> {
        let bytes = tokio::fs::read(self.path(key, size)).await.ok()?;
        Some(Thumbnail {
            media_type: Some(MEDIA_TYPE.to_owned()),
            data: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        })
    }

    async fn failed_lately(&self, key: &str) -> bool {
        let Ok(metadata) = tokio::fs::metadata(self.failed_path(key)).await else {
            return false;
        };
        metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < RETRY_AFTER)
    }

    /// Reads the file into a job of its own, draws it there and keeps its pictures. The job's
    /// folder goes when this ends, however it ends.
    async fn draw(&self, drawing: &Drawing<'_>, recipe: Recipe, key: &str) -> anyhow::Result<()> {
        let job = tempfile::Builder::new()
            .prefix("job")
            .tempdir_in(&self.work)
            .context("making the job's folder")?;
        let input = job
            .path()
            .join(recipe.input_name(&drawing.name, drawing.media_type.as_deref()));
        drawing
            .origin
            .save(recipe, &input)
            .await
            .context("reading the file")?;
        self.renderer.draw(recipe, job.path(), &input).await?;
        for size in [ThumbnailSize::Small, ThumbnailSize::Large] {
            keep(&job.path().join(file_of(size)), &self.path(key, size)).await?;
        }
        Ok(())
    }
}

fn file_of(size: ThumbnailSize) -> &'static str {
    match size {
        ThumbnailSize::Small => SMALL,
        ThumbnailSize::Large => LARGE,
    }
}

/// Moves a finished picture in whole, by a rename on the keeping disk.
async fn keep(from: &Path, to: &Path) -> anyhow::Result<()> {
    let partial = to.with_extension("partial");
    tokio::fs::copy(from, &partial)
        .await
        .with_context(|| format!("keeping {}", to.display()))?;
    tokio::fs::rename(&partial, to).await?;
    Ok(())
}

impl Origin<'_> {
    fn size(&self) -> u64 {
        match self {
            Self::Lore { size, .. } | Self::Artifact { size, .. } => *size,
        }
    }

    async fn save(&self, recipe: Recipe, to: &Path) -> anyhow::Result<()> {
        let ends = (recipe.reads_from_ends() && self.size() > HEAD + TAIL).then_some(Ends {
            size: self.size(),
            head: HEAD,
            tail: TAIL,
        });
        anyhow::ensure!(
            ends.is_some() || self.size() <= MAX_WHOLE,
            "the file is too large to read whole"
        );
        match self {
            Self::Lore {
                reader,
                repository,
                branch,
                path,
                ..
            } => {
                match ends {
                    Some(ends) => {
                        reader.save_ends(repository, branch, path, ends, to).await?;
                    }
                    None => reader.save(repository, branch, path, to).await?,
                }
                Ok(())
            }
            Self::Artifact {
                client, artifact, ..
            } => {
                let mut file = tokio::fs::File::create(to).await?;
                match ends {
                    Some(ends) => {
                        file.set_len(ends.size).await?;
                        for (first, last) in ends.ranges() {
                            file.seek(std::io::SeekFrom::Start(first)).await?;
                            let downloaded = client
                                .download(*artifact)
                                .range(first..=last)
                                .await
                                .map_err(|error| anyhow::anyhow!("{error}"))?;
                            let mut body = downloaded.body.into_reader();
                            tokio::io::copy(&mut body, &mut file).await?;
                        }
                    }
                    None => {
                        let downloaded = client
                            .download(*artifact)
                            .await
                            .map_err(|error| anyhow::anyhow!("{error}"))?;
                        let mut body = downloaded.body.into_reader();
                        tokio::io::copy(&mut body, &mut file).await?;
                    }
                }
                Ok(())
            }
        }
    }
}

fn artifact_id(artifact: &ArtifactRef) -> Result<ArtifactId, ThumbnailError> {
    artifact
        .as_str()
        .parse()
        .map_err(|_| ThumbnailError::Artifact("not an artifact's identifier".to_owned()))
}

/// A stage is the USD server's to draw, and it keeps what it draws. A stage it cannot draw, or no
/// USD server, leaves none.
async fn scene(
    reader: &Reader,
    repository: &RepositoryId,
    branch: &BranchName,
    path: &FilePath,
    size: ThumbnailSize,
) -> Thumbnail {
    let none = Thumbnail::default();
    let asked = (|| {
        Some(usd::ThumbnailRequest {
            repository: usd::RepositoryId::parse(repository.as_str()).ok()?,
            branch: usd::BranchName::parse(branch.as_str()).ok()?,
            path: usd::StagePath::parse(path.as_str()).ok()?,
            size: match size {
                ThumbnailSize::Small => usd::ThumbnailSize::Small,
                ThumbnailSize::Large => usd::ThumbnailSize::Large,
            },
        })
    })();
    let Some(asked) = asked else {
        return none;
    };
    match reader.client.call(asked).await {
        Ok(drawn) => Thumbnail {
            media_type: drawn.media_type,
            data: drawn.data,
        },
        Err(error) => {
            tracing::warn!(%error, "the USD server drew no picture");
            none
        }
    }
}

/// The name a file's pictures are kept under.
fn key(content: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(RECIPE);
    hash.update([0]);
    hash.update(content);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumbnails(folder: &Path) -> Thumbnails {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Thumbnails::new(
            folder.join("kept"),
            folder.join("work"),
            PathBuf::from("/bin/false"),
            reqwest::Client::builder().build().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_key_is_the_content_and_a_safe_name() {
        let key_of = key("lore\0abc");
        assert_eq!(key_of, key("lore\0abc"));
        assert_ne!(key_of, key("lore\0abd"));
        assert_ne!(key_of, key("artifact\0abc"));
        assert_eq!(key_of.len(), 64);
        assert!(key_of.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn jobs_left_by_another_process_are_gone_at_the_start() {
        let folder = tempfile::tempdir().unwrap();
        let left = folder.path().join("work/job1/input.pdf");
        std::fs::create_dir_all(left.parent().unwrap()).unwrap();
        std::fs::write(&left, b"x").unwrap();
        thumbnails(folder.path());
        assert!(!left.exists());
        assert!(folder.path().join("work").is_dir());
    }

    #[tokio::test]
    async fn a_failure_is_left_for_a_while_and_a_kept_picture_is_answered() {
        let folder = tempfile::tempdir().unwrap();
        let thumbnails = thumbnails(folder.path());
        assert!(!thumbnails.failed_lately("k").await);
        std::fs::write(thumbnails.failed_path("k"), b"").unwrap();
        assert!(thumbnails.failed_lately("k").await);

        assert!(
            thumbnails
                .kept_picture("k", ThumbnailSize::Small)
                .await
                .is_none()
        );
        std::fs::write(thumbnails.path("k", ThumbnailSize::Small), [1, 2, 3]).unwrap();
        let found = thumbnails
            .kept_picture("k", ThumbnailSize::Small)
            .await
            .unwrap();
        assert_eq!(found.data.as_deref(), Some("AQID"));
        assert_eq!(found.media_type.as_deref(), Some("image/webp"));
    }

    #[tokio::test]
    async fn a_failed_step_leaves_no_picture() {
        let folder = tempfile::tempdir().unwrap();
        let thumbnails = thumbnails(folder.path());
        let job = tempfile::tempdir().unwrap();
        let input = job.path().join("input.png");
        std::fs::write(&input, b"not a picture").unwrap();
        assert!(
            thumbnails
                .renderer
                .draw(Recipe::Image, job.path(), &input)
                .await
                .is_err()
        );
        assert!(!job.path().join(SMALL).exists());
    }
}

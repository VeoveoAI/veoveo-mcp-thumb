//! Drawing a file: standard programs, unmodified, each in its own confined process with a time
//! limit, and nothing else. The commands come from Thumbelina (Apache-2.0,
//! <https://hub.docker.com/r/sciactive/thumbelina>), except 3D, which F3D draws on the GPU.
//! Every recipe leaves `page.png` in the job's folder, and one last step makes both pictures.
use std::{
    ffi::OsString,
    os::unix::process::CommandExt as _,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, bail};
use rustix::process::{Pid, Signal, kill_process_group};

use crate::contract::FileKind;

/// The picture every recipe leaves in the job's folder.
const PAGE: &str = "page.png";
/// The side a page is drawn at, and at most the large picture's.
const LARGE_SIDE: u32 = 1280;
/// The small picture's side.
const SMALL_SIDE: u32 = 480;
/// The pictures' quality.
const QUALITY: &str = "78";
/// The finished pictures in the job's folder.
pub const SMALL: &str = "small.webp";
pub const LARGE: &str = "large.webp";
/// The longest a document may take to convert.
const CONVERT_TIMEOUT: Duration = Duration::from_secs(90);
/// The longest any other step may take.
const STEP_TIMEOUT: Duration = Duration::from_secs(60);
/// How much of a failed step's error output a log line holds.
const MAX_LOGGED_ERROR: usize = 512;

/// How a file is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    Image,
    Vector,
    Video,
    Audio,
    Pdf,
    /// Anything LibreOffice opens, as what it is.
    Office,
    /// Text LibreOffice reads as plain text.
    Text,
    Model,
}

impl Recipe {
    /// The recipe for a file called `name`, or none when there is no drawing it.
    pub fn of(name: &str, media_type: Option<&str>) -> Option<Self> {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        Some(match extension.as_deref() {
            Some("svg") => Self::Vector,
            Some("pdf") => Self::Pdf,
            Some(
                "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "csv" | "tsv" | "ppt"
                | "pptx" | "odp" | "html" | "htm" | "xhtml",
            ) => Self::Office,
            // A DWG is listed as a model, but nothing here reads one, and a stage goes to USD.
            Some("dwg") => return None,
            _ => match FileKind::of(name, media_type) {
                FileKind::Image => Self::Image,
                FileKind::Video => Self::Video,
                FileKind::Audio => Self::Audio,
                FileKind::Model => Self::Model,
                FileKind::Document if media_type == Some("application/pdf") => Self::Pdf,
                FileKind::Document => Self::Text,
                FileKind::Scene | FileKind::Other => return None,
            },
        })
    }

    /// Whether a frame can be read from a file's two ends, so a large one is not read whole.
    pub fn reads_from_ends(self) -> bool {
        matches!(self, Self::Video | Self::Audio)
    }

    /// What the job's input is called. Its name is never the person's, so that no program reads
    /// one as an option or a format's prefix, and its extension is what the program goes by.
    pub fn input_name(self, name: &str, media_type: Option<&str>) -> String {
        if self == Self::Text {
            return "input.txt".to_owned();
        }
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .filter(|extension| {
                !extension.is_empty()
                    && extension.len() <= 8
                    && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
            });
        let extension = extension.or_else(|| {
            match media_type? {
                "application/pdf" => Some("pdf"),
                "image/png" => Some("png"),
                "image/jpeg" => Some("jpg"),
                "image/webp" => Some("webp"),
                "video/mp4" => Some("mp4"),
                _ => None,
            }
            .map(str::to_owned)
        });
        extension.map_or_else(
            || "input".to_owned(),
            |extension| format!("input.{extension}"),
        )
    }
}

/// The confined steps of drawing files.
#[derive(Clone)]
pub struct Renderer {
    /// This program, whose `confine` subcommand starts each step.
    program: PathBuf,
}

impl Renderer {
    pub fn new(program: PathBuf) -> Self {
        Self { program }
    }

    /// Draws `input`, in the folder `job`, into its two pictures.
    pub async fn draw(&self, recipe: Recipe, job: &Path, input: &Path) -> anyhow::Result<()> {
        match recipe {
            Recipe::Image => {
                self.image(job, input).await?;
            }
            Recipe::Vector => {
                let side = LARGE_SIDE.to_string();
                self.step(
                    job,
                    false,
                    "rsvg-convert",
                    &[
                        "--keep-aspect-ratio".into(),
                        "--width".into(),
                        side.into(),
                        "--output".into(),
                        job.join(PAGE).into(),
                        input.into(),
                    ],
                    STEP_TIMEOUT,
                )
                .await?;
            }
            Recipe::Video => self.video(job, input).await?,
            Recipe::Audio => self.audio(job, input).await?,
            Recipe::Pdf => self.pdf(job, input).await?,
            Recipe::Office | Recipe::Text => self.office(job, input).await?,
            Recipe::Model => self.model(job, input).await?,
        }
        self.pictures(job).await
    }

    async fn image(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        // The first frame or layer. `[0]` is the program's own syntax, on a name this server chose.
        let mut first = input.as_os_str().to_owned();
        first.push("[0]");
        self.step(
            job,
            false,
            "convert",
            &[
                first,
                "-auto-orient".into(),
                "-resize".into(),
                format!("{LARGE_SIDE}x{LARGE_SIDE}>").into(),
                job.join(PAGE).into(),
            ],
            STEP_TIMEOUT,
        )
        .await
    }

    async fn video(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        let scale = format!(
            "thumbnail=24,scale={LARGE_SIDE}:{LARGE_SIDE}:force_original_aspect_ratio=decrease"
        );
        let frame = |skip: &[&str]| -> Vec<OsString> {
            let mut args: Vec<OsString> = vec!["-nostdin".into(), "-v".into(), "error".into()];
            args.extend(skip.iter().map(OsString::from));
            args.extend([
                "-i".into(),
                input.into(),
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                OsString::from(&scale),
                "-y".into(),
                job.join(PAGE).into(),
            ]);
            args
        };
        // A second in, past a fade from black; a clip shorter than that from its start.
        let skipped = self
            .step(job, false, "ffmpeg", &frame(&["-ss", "1"]), STEP_TIMEOUT)
            .await;
        if skipped.is_ok() && job.join(PAGE).exists() {
            return Ok(());
        }
        self.step(job, false, "ffmpeg", &frame(&[]), STEP_TIMEOUT)
            .await
    }

    async fn audio(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        // Its cover, when it has one.
        let cover: Vec<OsString> = vec![
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-i".into(),
            input.into(),
            "-an".into(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            format!("scale={LARGE_SIDE}:{LARGE_SIDE}:force_original_aspect_ratio=decrease").into(),
            "-y".into(),
            job.join(PAGE).into(),
        ];
        let covered = self.step(job, false, "ffmpeg", &cover, STEP_TIMEOUT).await;
        if covered.is_ok() && job.join(PAGE).exists() {
            return Ok(());
        }
        // Otherwise the shape of its first minutes.
        let wave: Vec<OsString> = vec![
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-t".into(),
            "300".into(),
            "-i".into(),
            input.into(),
            "-filter_complex".into(),
            "aformat=channel_layouts=mono,showwavespic=s=1280x640:colors=#6f8fd8".into(),
            "-frames:v".into(),
            "1".into(),
            "-y".into(),
            job.join(PAGE).into(),
        ];
        self.step(job, false, "ffmpeg", &wave, STEP_TIMEOUT).await
    }

    async fn pdf(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        self.step(
            job,
            false,
            "pdftoppm",
            &[
                "-png".into(),
                "-singlefile".into(),
                "-f".into(),
                "1".into(),
                "-l".into(),
                "1".into(),
                "-scale-to".into(),
                LARGE_SIDE.to_string().into(),
                input.into(),
                job.join("page").into(),
            ],
            STEP_TIMEOUT,
        )
        .await
    }

    async fn office(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        let profile = format!("-env:UserInstallation=file://{}", job.join("lo").display());
        let out = job.join("pdf");
        self.step(
            job,
            false,
            "soffice",
            &[
                "--headless".into(),
                "--norestore".into(),
                profile.into(),
                "--convert-to".into(),
                // The first page only, however long the document is.
                r#"pdf:writer_pdf_Export:{"PageRange":{"type":"string","value":"1"}}"#.into(),
                "--outdir".into(),
                (&out).into(),
                input.into(),
            ],
            CONVERT_TIMEOUT,
        )
        .await?;
        let Some(stem) = input.file_stem() else {
            bail!("the job's input has no name");
        };
        let mut pdf = OsString::from(stem);
        pdf.push(".pdf");
        self.pdf(job, &out.join(pdf)).await
    }

    async fn model(&self, job: &Path, input: &Path) -> anyhow::Result<()> {
        self.step(
            job,
            true,
            "f3d",
            &[
                "--no-config".into(),
                "--verbose=quiet".into(),
                "--rendering-backend=egl".into(),
                format!("--resolution={LARGE_SIDE},{LARGE_SIDE}").into(),
                "--no-background".into(),
                "--camera-azimuth-angle=35".into(),
                "--camera-elevation-angle=25".into(),
                "--camera-zoom-factor=0.7".into(),
                {
                    let mut output = OsString::from("--output=");
                    output.push(job.join(PAGE));
                    output
                },
                input.into(),
            ],
            STEP_TIMEOUT,
        )
        .await
    }

    /// The page as both pictures.
    async fn pictures(&self, job: &Path) -> anyhow::Result<()> {
        let page = job.join(PAGE);
        anyhow::ensure!(page.exists(), "the recipe drew nothing");
        self.step(
            job,
            false,
            "convert",
            &[
                page.into(),
                "-resize".into(),
                format!("{LARGE_SIDE}x{LARGE_SIDE}>").into(),
                "-strip".into(),
                "-quality".into(),
                QUALITY.into(),
                "-write".into(),
                job.join(LARGE).into(),
                "-resize".into(),
                format!("{SMALL_SIDE}x{SMALL_SIDE}>").into(),
                job.join(SMALL).into(),
            ],
            STEP_TIMEOUT,
        )
        .await
    }

    /// One step: `program` in its own process group, confined, with the job as its home and
    /// nothing of the server's environment. Whatever it left running goes with it.
    async fn step(
        &self,
        job: &Path,
        gpu: bool,
        program: &str,
        args: &[OsString],
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let mut command = std::process::Command::new(&self.program);
        command.arg("confine").arg(job);
        if gpu {
            command.arg("--gpu");
        }
        command
            .arg("--")
            .arg(program)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", job)
            .env("TMPDIR", job)
            .env("MAGICK_TMPDIR", job)
            .env("LANG", "C.UTF-8")
            .current_dir(job)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .process_group(0);
        if gpu {
            command
                .env("NVIDIA_VISIBLE_DEVICES", "all")
                .env("NVIDIA_DRIVER_CAPABILITIES", "all");
        }
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let child = command
            .spawn()
            .with_context(|| format!("starting {program}"))?;
        let group = child.id().and_then(|id| Pid::from_raw(id.cast_signed()));
        let finished = tokio::time::timeout(timeout, child.wait_with_output()).await;
        if let Some(group) = group {
            // Already gone, as a rule, and then this has nothing to say.
            let _ = kill_process_group(group, Signal::KILL);
        }
        let finished = finished
            .map_err(|_| anyhow::anyhow!("{program} took longer than {timeout:?}"))?
            .with_context(|| format!("waiting for {program}"))?;
        if finished.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&finished.stderr);
        let start = stderr.len().saturating_sub(MAX_LOGGED_ERROR);
        let tail = stderr.get(start..).unwrap_or_default();
        bail!("{program} exited with {}: {}", finished.status, tail.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_follows_the_file() {
        for (name, recipe) in [
            ("a.docx", Recipe::Office),
            ("a.XLSX", Recipe::Office),
            ("a.pptx", Recipe::Office),
            ("index.html", Recipe::Office),
            ("a.pdf", Recipe::Pdf),
            ("a.md", Recipe::Text),
            ("main.rs", Recipe::Text),
            ("a.png", Recipe::Image),
            ("a.psd", Recipe::Image),
            ("a.svg", Recipe::Vector),
            ("a.mp4", Recipe::Video),
            ("a.flac", Recipe::Audio),
            ("a.glb", Recipe::Model),
            ("a.step", Recipe::Model),
        ] {
            assert_eq!(Recipe::of(name, None), Some(recipe), "{name}");
        }
        assert_eq!(Recipe::of("a.usdz", None), None);
        assert_eq!(Recipe::of("a.dwg", None), None);
        assert_eq!(Recipe::of("a.bin", None), None);
        assert_eq!(Recipe::of("clip", Some("video/mp4")), Some(Recipe::Video));
        assert_eq!(
            Recipe::of("paper", Some("application/pdf")),
            Some(Recipe::Pdf)
        );
    }

    #[test]
    fn an_input_is_never_named_by_the_person() {
        let name = |recipe: Recipe, file: &str| recipe.input_name(file, None);
        assert_eq!(name(Recipe::Office, "my report.DOCX"), "input.docx");
        assert_eq!(name(Recipe::Text, "notes.md"), "input.txt");
        assert_eq!(name(Recipe::Image, "-write x.png"), "input.png");
        assert_eq!(name(Recipe::Image, "a.p/../ng"), "input");
        assert_eq!(name(Recipe::Image, "noextension"), "input");
        assert_eq!(
            Recipe::Pdf.input_name("paper", Some("application/pdf")),
            "input.pdf"
        );
    }

    #[test]
    fn only_timed_media_is_read_from_its_ends() {
        assert!(Recipe::Video.reads_from_ends());
        assert!(Recipe::Audio.reads_from_ends());
        assert!(!Recipe::Pdf.reads_from_ends());
        assert!(!Recipe::Model.reads_from_ends());
    }
}

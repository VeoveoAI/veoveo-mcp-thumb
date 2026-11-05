//! The tools: listing what a repository holds and drawing a picture of a file.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ArtifactRef, BranchName, FilePath, FilesProvider, RepositoryId};

/// The branch `list_files` reads when none is named.
pub const DEFAULT_BRANCH: &str = "main";

/// The arguments of `list_files`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[veoveo_hub_sdk::tool(provider = FilesProvider, output = FileListing, name = "list_files")]
pub struct ListFilesRequest {
    /// The repository to list. Without one, lists the repositories you can read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryId>,
    /// The branch to read; `main` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<BranchName>,
    /// The folder to list; the repository's top by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<FilePath>,
}

/// A repository the caller can read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositorySummary {
    pub id: RepositoryId,
    pub name: String,
}

/// What a name in a folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    File,
    Directory,
    Link,
}

/// What sort of thing a file holds, by its name or its media type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Document,
    Model,
    Scene,
    Other,
}

impl FileKind {
    /// The kind of a file called `name`, whose `media_type` may also say.
    pub fn of(name: &str, media_type: Option<&str>) -> Self {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        match extension.as_deref() {
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif"
                | "avif" | "ico" | "psd" | "svg" | "tga" | "exr" | "hdr" | "jp2",
            ) => Self::Image,
            Some(
                "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "flv" | "mpg" | "mpeg"
                | "ogv" | "3gp",
            ) => Self::Video,
            Some(
                "mp3" | "wav" | "flac" | "ogg" | "oga" | "m4a" | "aac" | "opus" | "wma" | "aiff",
            ) => Self::Audio,
            Some("usd" | "usda" | "usdc" | "usdz") => Self::Scene,
            Some(
                "glb" | "gltf" | "fbx" | "obj" | "stl" | "ply" | "dae" | "3ds" | "off" | "step"
                | "stp" | "iges" | "igs" | "brep" | "3mf" | "dxf" | "abc" | "wrl" | "vrml" | "ifc"
                | "drc" | "pts" | "dwg",
            ) => Self::Model,
            Some(
                "pdf" | "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "csv" | "tsv"
                | "ppt" | "pptx" | "odp" | "md" | "markdown" | "txt" | "log" | "html" | "htm"
                | "xhtml" | "json" | "yaml" | "yml" | "toml" | "xml" | "rs" | "py" | "js" | "ts"
                | "tsx" | "jsx" | "c" | "h" | "cpp" | "hpp" | "go" | "java" | "sh" | "css" | "sql",
            ) => Self::Document,
            _ => match media_type.and_then(|media| media.split('/').next()) {
                Some("image") => Self::Image,
                Some("video") => Self::Video,
                Some("audio") => Self::Audio,
                Some("text") => Self::Document,
                _ if media_type == Some("application/pdf") => Self::Document,
                _ => Self::Other,
            },
        }
    }
}

/// One name in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    /// The bytes of a file or of all a folder holds, as the Hub lists them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// What a file holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FileKind>,
}

/// What `list_files` answers: the repositories, or a folder of one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[derive(veoveo_hub_sdk::SingleOutput)]
pub struct FileListing {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<RepositorySummary>,
    /// The repository `entries` belong to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryId>,
    /// The branch `entries` were read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<BranchName>,
    /// The folder `entries` are in; none is the repository's top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<FilePath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<FileEntry>,
}

/// A file a person may read, in either place files live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "lowercase", deny_unknown_fields)]
pub enum FileSource {
    /// A file in a Lore repository.
    Lore {
        repository: RepositoryId,
        branch: BranchName,
        path: FilePath,
    },
    /// An artifact.
    Artifact { artifact: ArtifactRef },
}

/// How large a picture to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    /// For a gallery's card: at most 480 pixels on a side.
    #[default]
    Small,
    /// For a closer look: at most 1280 pixels on a side.
    Large,
}

/// The arguments of `thumbnail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[veoveo_hub_sdk::tool(provider = FilesProvider, output = Thumbnail, name = "thumbnail")]
pub struct ThumbnailRequest {
    pub source: FileSource,
    #[serde(default)]
    pub size: ThumbnailSize,
}

/// A file's picture, or none when it could not be drawn, with what the file holds either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[derive(veoveo_hub_sdk::SingleOutput)]
pub struct Thumbnail {
    pub kind: FileKind,
    /// The picture's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The picture, in base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_comes_from_the_name_first_and_the_media_type_after() {
        assert_eq!(FileKind::of("a/b/report.DOCX", None), FileKind::Document);
        assert_eq!(FileKind::of("fox.glb", None), FileKind::Model);
        assert_eq!(FileKind::of("bird.usdz", None), FileKind::Scene);
        assert_eq!(FileKind::of("clip", Some("video/mp4")), FileKind::Video);
        assert_eq!(
            FileKind::of("paper", Some("application/pdf")),
            FileKind::Document
        );
        assert_eq!(FileKind::of("blob.bin", None), FileKind::Other);
    }

    #[test]
    fn a_source_says_where_it_lives() {
        let source: FileSource = serde_json::from_value(serde_json::json!({
            "from": "lore", "repository": "r", "branch": "main", "path": "a/b.png"
        }))
        .unwrap();
        assert!(matches!(source, FileSource::Lore { .. }));
        assert!(
            serde_json::from_value::<FileSource>(serde_json::json!({
                "from": "lore", "repository": "r", "branch": "main", "path": "../b.png"
            }))
            .is_err()
        );
    }
}

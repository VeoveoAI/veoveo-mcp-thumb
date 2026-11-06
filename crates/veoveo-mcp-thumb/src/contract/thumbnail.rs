//! The tools: a thumbnail of a file, and what is ready of many files by one request.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ArtifactRef, BranchName, FilePath, RepositoryId, ThumbProvider};

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

/// How large a thumbnail to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    /// At most 480 pixels on a side.
    #[default]
    Small,
    /// At most 1280 pixels on a side.
    Large,
}

/// The arguments of `thumbnail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[veoveo_hub_sdk::tool(provider = ThumbProvider, output = Thumbnail, name = "thumbnail")]
pub struct ThumbnailRequest {
    pub source: FileSource,
    #[serde(default)]
    pub size: ThumbnailSize,
}

/// A file's thumbnail, or none when one could not be made.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[derive(veoveo_hub_sdk::SingleOutput)]
pub struct Thumbnail {
    /// The picture's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The picture, in base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// The most files one `ready_thumbnails` names: about a screen of pictures.
pub const MOST_READY_FILES: usize = 64;

/// The arguments of `ready_thumbnails`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[veoveo_hub_sdk::tool(provider = ThumbProvider, output = ReadyThumbnails, name = "ready_thumbnails")]
pub struct ReadyThumbnailsRequest {
    /// The files, at most 64.
    pub sources: Vec<FileSource>,
    #[serde(default)]
    pub size: ThumbnailSize,
}

/// What is ready of one file: its picture, or none, or `pending` when `thumbnail` has more to
/// say, because it can still draw one or this could not tell.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyThumbnail {
    /// The picture's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The picture, in base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
}

/// What `ready_thumbnails` answers, one for each file, in their order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[derive(veoveo_hub_sdk::SingleOutput)]
pub struct ReadyThumbnails {
    pub thumbnails: Vec<ReadyThumbnail>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

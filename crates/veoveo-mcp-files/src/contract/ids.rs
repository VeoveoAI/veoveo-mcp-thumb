//! The server's checked values: what names a repository, a branch, a file and an artifact. Each
//! admits its input once, so internal code never handles a raw string where one of these is meant.
//! Lore and Artifact decide what exists; these refuse what could not be a name.
use std::{borrow::Cow, fmt};

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

/// The most bytes any of these values holds.
const MAX_BYTES: usize = 1024;

/// A value that failed admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FilesIdError {
    #[error("a repository is 1 to 128 characters of letters, digits, `-` and `_`")]
    Repository,
    #[error("a branch is a name of 1 to 1024 bytes without control characters")]
    Branch,
    #[error("a path is relative and `/`-separated, with no empty, `.` or `..` segment")]
    Path,
    #[error("an artifact is identified by a UUID")]
    Artifact,
}

fn no_controls(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_BYTES && !value.chars().any(char::is_control)
}

macro_rules! checked_string {
    ($(#[$meta:meta])* $name:ident, $describe:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = FilesIdError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({"type": "string", "description": $describe})
            }
        }
    };
}

checked_string!(
    /// A Lore repository, as the Hub names it.
    RepositoryId,
    "A Lore repository's identifier, from list_files."
);
checked_string!(
    /// A branch of a repository.
    BranchName,
    "A branch of the repository."
);
checked_string!(
    /// A file or a folder in a repository, relative to its top.
    FilePath,
    "A path in the repository, such as docs/report.docx."
);
checked_string!(
    /// An artifact.
    ArtifactRef,
    "An artifact's identifier."
);

impl RepositoryId {
    pub fn parse(value: impl Into<String>) -> Result<Self, FilesIdError> {
        let value = value.into();
        let admitted = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        admitted
            .then_some(Self(value))
            .ok_or(FilesIdError::Repository)
    }
}

impl BranchName {
    pub fn parse(value: impl Into<String>) -> Result<Self, FilesIdError> {
        let value = value.into();
        no_controls(&value)
            .then_some(Self(value))
            .ok_or(FilesIdError::Branch)
    }
}

impl FilePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, FilesIdError> {
        let value = value.into();
        let segments = value
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | "..") && !segment.contains('\\'));
        (no_controls(&value) && segments)
            .then_some(Self(value))
            .ok_or(FilesIdError::Path)
    }

    /// The path's segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The last segment.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
}

impl ArtifactRef {
    pub fn parse(value: impl Into<String>) -> Result<Self, FilesIdError> {
        let value = value.into();
        let admitted = value.len() == 36
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
        admitted
            .then_some(Self(value))
            .ok_or(FilesIdError::Artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_cannot_climb_or_hide_a_segment() {
        for path in ["a/../b", "../a", "a//b", "/a", "a/", "a\\b", "a/./b", ""] {
            assert!(FilePath::parse(path).is_err(), "{path}");
        }
        let path = FilePath::parse("docs/my report.docx").unwrap();
        assert_eq!(path.file_name(), "my report.docx");
        assert_eq!(path.segments().count(), 2);
    }

    #[test]
    fn a_repository_is_one_url_segment() {
        assert!(RepositoryId::parse("01a1ac8c-fd47").is_ok());
        for repository in ["", "a/b", "a b", "a%2fb"] {
            assert!(RepositoryId::parse(repository).is_err(), "{repository}");
        }
    }

    #[test]
    fn an_artifact_is_a_uuid_shape() {
        assert!(ArtifactRef::parse("01a1acb5-2ac9-7b03-be1b-14b1e93ab968").is_ok());
        assert!(ArtifactRef::parse("../../etc/passwd").is_err());
    }

    #[test]
    fn serde_admits_only_what_parse_does() {
        assert!(serde_json::from_str::<FilePath>("\"../a\"").is_err());
        assert_eq!(
            serde_json::from_str::<FilePath>("\"a/b\"")
                .unwrap()
                .as_str(),
            "a/b"
        );
    }
}

//! The server's public types. Consumers select `default-features = false,
//! features = ["contract"]`, which builds without a runtime dependency.

mod calls;
pub use calls::FilesProvider;
mod files;
mod ids;
mod permissions;
mod resource;

pub use files::{
    DEFAULT_BRANCH, EntryType, FileEntry, FileKind, FileListing, FileSource, ListFilesRequest,
    RepositorySummary, Thumbnail, ThumbnailRequest, ThumbnailSize,
};
pub use ids::{ArtifactRef, BranchName, FilePath, FilesIdError, RepositoryId};
pub use permissions::{FilesPermission, FilesScope};
pub use resource::{FilesDocument, FilesResource, FilesResourceError};

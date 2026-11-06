//! The server's public types. Consumers select `default-features = false,
//! features = ["contract"]`, which builds without a runtime dependency.

mod calls;
pub use calls::ThumbProvider;
mod ids;
mod permissions;
mod resource;
mod thumbnail;

pub use ids::{ArtifactRef, BranchName, FilePath, RepositoryId, ThumbIdError};
pub use permissions::{ThumbPermission, ThumbScope};
pub use resource::{ThumbDocument, ThumbResource, ThumbResourceError};
pub use thumbnail::{
    FileSource, MOST_READY_FILES, ReadyThumbnail, ReadyThumbnails, ReadyThumbnailsRequest,
    Thumbnail, ThumbnailRequest, ThumbnailSize,
};

//! The server's OAuth scope and permissions. Each permission names one action on one
//! resource type, with the access kind and risks the `ai.veoveo/authorization`
//! declaration carries; installation roles grant them and the Hub forwards the
//! granted ones.
use veoveo_hub_sdk::registration::{Access, PermissionDefinition, Risk};
use veoveo_types::{PermissionName, Vocabulary};

/// The OAuth scope that names the server's namespace. Scopes are coarse; the
/// permissions carry fine-grained access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Vocabulary)]
#[vocabulary(scope)]
pub enum ThumbScope {
    #[vocabulary(rename = "thumb")]
    Files,
}

/// The server's permissions.
/// ```compile_fail
/// use veoveo_mcp_thumb::contract::ThumbPermission;
/// fn requires(_: ThumbPermission) {}
/// requires("thumb.thumbnails.read");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Vocabulary)]
pub enum ThumbPermission {
    /// Get a thumbnail of a file you can read.
    #[vocabulary(rename = "thumb.thumbnails.read")]
    ThumbnailsRead,
}

impl ThumbPermission {
    /// The permission's wire name, as declared and as granted.
    pub fn name(self) -> PermissionName {
        PermissionName::parse(self.as_str()).expect("declared thumb permission spelling")
    }
}

impl PermissionDefinition for ThumbPermission {
    fn resource(self) -> &'static str {
        "thumb.thumbnail"
    }

    fn access(self) -> Access {
        Access::Read
    }

    fn risks(self) -> &'static [Risk] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_follow_the_naming_form() {
        for permission in ThumbPermission::ALL {
            let name = permission.as_str();
            let mut parts = name.split('.');
            assert_eq!(parts.next(), Some("thumb"), "{name}");
            assert!(parts.next().is_some_and(|plural| !plural.is_empty()));
            assert!(parts.next().is_some_and(|verb| !verb.is_empty()));
            assert_eq!(parts.next(), None, "{name}");
            assert_eq!(permission.name().as_str(), name);
        }
        assert_eq!(ThumbScope::Files.as_str(), "thumb");
    }
}

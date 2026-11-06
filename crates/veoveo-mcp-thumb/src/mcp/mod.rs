//! One typed route table binds requests, outputs, discovery and current permissions.
mod declarations;
mod requirements;
mod setup;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use std::sync::LazyLock;
use veoveo_hub_sdk::{
    hosting::{ClientContext, DomainServer, RoutePolicy, Routes, ToolError, hub_identity},
    server_contract::McpServerSetup,
};
use veoveo_mcp_artifact::contract::ArtifactPermission;
use veoveo_mcp_usd::contract::UsdPermission;

use crate::{
    contract::{
        MOST_READY_FILES, ReadyThumbnails, ReadyThumbnailsRequest, ThumbPermission, Thumbnail,
        ThumbnailRequest,
    },
    hub::{Hub, Reader},
    thumbnail::Thumbnails,
};

pub use declarations::{REGISTRATION, registration};
pub use requirements::AUTHORIZATION;
use setup::SERVER_SETUP;
pub use setup::ThumbContract;

pub fn force_setup() {
    LazyLock::force(&SERVER_SETUP);
}

#[derive(Clone)]
pub struct ThumbMcp {
    hub: Hub,
    thumbnails: Thumbnails,
}

impl ThumbMcp {
    pub fn new(hub: Hub, thumbnails: Thumbnails) -> Self {
        Self { hub, thumbnails }
    }

    fn reader(&self, context: &RequestContext<RoleServer>) -> Result<Reader, ToolError> {
        let identity = hub_identity(context).map_err(|error: McpError| {
            ToolError::Rejected(format!("the caller is not known: {}", error.message))
        })?;
        Ok(Reader {
            hub: self.hub.clone(),
            client: context.client().map_err(|error: McpError| {
                ToolError::Rejected(format!("no calls for the caller: {}", error.message))
            })?,
            profile: identity.profile,
        })
    }

    async fn thumbnail(
        &self,
        request: ThumbnailRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<Thumbnail, ToolError> {
        self.thumbnails
            .thumbnail(&self.reader(&context)?, &request)
            .await
            .map_err(|error| ToolError::Rejected(format!("reading the file: {error}")))
    }

    async fn ready_thumbnails(
        &self,
        request: ReadyThumbnailsRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadyThumbnails, ToolError> {
        if request.sources.len() > MOST_READY_FILES {
            return Err(ToolError::Rejected(format!(
                "at most {MOST_READY_FILES} files at a time"
            )));
        }
        Ok(ReadyThumbnails {
            thumbnails: self
                .thumbnails
                .ready(&self.reader(&context)?, &request.sources, request.size)
                .await,
        })
    }
}

fn policy() -> RoutePolicy {
    RoutePolicy::permission(ThumbPermission::ThumbnailsRead).expect("the server's permission")
}

static ROUTES: LazyLock<Routes<ThumbMcp>> = LazyLock::new(|| {
    Routes::<ThumbMcp>::builder(policy())
        .uses(ArtifactPermission::ArtifactsRead.name())
        .uses(UsdPermission::StagesRead.name())
        .calls_hub()
        .tool::<ThumbnailRequest, _, _>(
            "Make a thumbnail of a file you can read in a Lore repository or Artifact, as base64, or say there is none. The first call for a file's content draws it; later ones answer from what was drawn, after asking whether you may still read it.",
            policy(),
            |domain, request, context| async move { domain.thumbnail(request, context).await },
        )
        .tool::<ReadyThumbnailsRequest, _, _>(
            "What is ready of many files you can read by one request: each one's kept thumbnail as base64, or none, or `pending` when `thumbnail` can still make one. It never draws.",
            policy(),
            |domain, request, context| async move {
                domain.ready_thumbnails(request, context).await
            },
        )
        .build()
});

impl DomainServer for ThumbMcp {
    type Contract = ThumbContract;
    fn setup() -> &'static McpServerSetup<ThumbContract> {
        &SERVER_SETUP
    }
    fn routes() -> &'static Routes<Self> {
        &ROUTES
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_setup_and_documents_admit() {
        std::sync::LazyLock::force(&super::SERVER_SETUP);
        assert!(super::setup::SERVER_DOCS.agent_manual().is_some());
    }

    #[test]
    fn the_routes_call_the_hub_and_declare_what_they_use() {
        let authorization = super::ROUTES.authorization();
        assert!(authorization.calls_hub);
    }
}

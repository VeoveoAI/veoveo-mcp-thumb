//! One typed route table binds requests, outputs, discovery and current permissions.
mod declarations;
mod requirements;
mod setup;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use std::sync::LazyLock;
use veoveo_hub_sdk::{
    apps::{AppHtml, AppUri, UiVisibility},
    calls::ToolCall,
    hosting::{
        AppBuilder, ClientContext, DomainServer, RoutePolicy, Routes, ToolError, hub_identity,
    },
    server_contract::McpServerSetup,
};
use veoveo_mcp_artifact::contract::ArtifactPermission;
use veoveo_mcp_usd::contract::UsdPermission;

use crate::{
    contract::{
        BranchName, DEFAULT_BRANCH, EntryType, FileEntry, FileKind, FileListing, FilesPermission,
        ListFilesRequest, RepositorySummary, Thumbnail, ThumbnailRequest,
    },
    hub::{EntryKind, Hub, HubError, Reader},
    thumbnail::Thumbnails,
};

pub use declarations::{REGISTRATION, registration};
pub use requirements::AUTHORIZATION;
pub use setup::FilesContract;
use setup::SERVER_SETUP;

/// The App's address.
pub const APP_URI: &str = "ui://files/browser";
/// The App's programmatic resource name.
const APP_NAME: &str = "browser";

pub fn force_setup() {
    LazyLock::force(&SERVER_SETUP);
}

#[derive(Clone)]
pub struct FilesMcp {
    hub: Hub,
    app: AppHtml,
    thumbnails: Thumbnails,
}

impl FilesMcp {
    pub fn new(hub: Hub, app: AppHtml, thumbnails: Thumbnails) -> Self {
        Self {
            hub,
            app,
            thumbnails,
        }
    }

    fn reader(&self, context: &RequestContext<RoleServer>) -> Result<Reader, McpError> {
        let identity = hub_identity(context)?;
        Ok(Reader {
            hub: self.hub.clone(),
            client: context.client()?,
            profile: identity.profile,
        })
    }

    async fn list_files(
        &self,
        request: ListFilesRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<FileListing, ToolError> {
        let reader = self.reader(&context)?;
        let Some(repository) = request.repository else {
            let mut repositories = reader
                .repositories()
                .await
                .map_err(|error| refused("listing your repositories", &error))?;
            repositories.sort_by_key(|repository| repository.name.to_lowercase());
            return Ok(FileListing {
                repositories: repositories
                    .into_iter()
                    .map(|repository| RepositorySummary {
                        id: repository.id,
                        name: repository.name,
                    })
                    .collect(),
                repository: None,
                branch: None,
                path: None,
                entries: vec![],
            });
        };
        let branch = match request.branch {
            Some(branch) => branch,
            None => BranchName::parse(DEFAULT_BRANCH).expect("the default branch's name"),
        };
        let listing = reader
            .listing(&repository, &branch, request.path.as_ref())
            .await
            .map_err(|error| refused("listing the folder", &error))?;
        let mut entries: Vec<FileEntry> = listing
            .entries
            .into_iter()
            .map(|entry| {
                let (entry_type, kind) = match entry.kind {
                    EntryKind::File => (EntryType::File, Some(FileKind::of(&entry.name, None))),
                    EntryKind::Directory => (EntryType::Directory, None),
                    EntryKind::Link => (EntryType::Link, None),
                };
                FileEntry {
                    name: entry.name,
                    entry_type,
                    size: entry.size,
                    kind,
                }
            })
            .collect();
        entries.sort_by_key(|entry| {
            (
                entry.entry_type != EntryType::Directory,
                entry.name.to_lowercase(),
            )
        });
        Ok(FileListing {
            repositories: vec![],
            repository: Some(repository),
            branch: Some(branch),
            path: request.path,
            entries,
        })
    }

    async fn thumbnail(
        &self,
        request: ThumbnailRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<Thumbnail, ToolError> {
        let reader = self.reader(&context)?;
        self.thumbnails
            .thumbnail(&reader, &request)
            .await
            .map_err(|error| ToolError::Rejected(format!("reading the file: {error}")))
    }
}

fn refused(what: &str, error: &HubError) -> ToolError {
    ToolError::Rejected(format!("{what}: {error}"))
}

fn policy() -> RoutePolicy {
    RoutePolicy::permission(FilesPermission::EntriesRead).expect("the server's permission")
}

fn app() -> AppUri {
    AppUri::parse(APP_URI).expect("the App's address")
}

static ROUTES: LazyLock<Routes<FilesMcp>> = LazyLock::new(|| {
    Routes::<FilesMcp>::builder(policy())
        .uses(ArtifactPermission::ArtifactsRead.name())
        .uses(UsdPermission::StagesRead.name())
        .calls_hub()
        .tool::<ListFilesRequest, _, _>(
            "List the Lore repositories you can read or, given a `repository`, what a folder of it holds on a branch: `main` and its top by default. Each file says what kind of thing it holds.",
            policy(),
            |domain, request, context| async move { domain.list_files(request, context).await },
        )
        .tool::<ThumbnailRequest, _, _>(
            "Draw a picture of a file you can read in a Lore repository or Artifact, for a gallery, or say what it holds when it has no picture. The first call for a file's content draws it; later ones answer from what was drawn, after asking whether you may still read it.",
            policy(),
            |domain, request, context| async move { domain.thumbnail(request, context).await },
        )
        .app(
            AppBuilder::new(app(), APP_NAME)
                .title("Files")
                .description("The Lore repositories you can read, as a gallery of their files. Choosing one shows a larger picture and opens it.")
                .policy(policy())
                .reader(|domain: FilesMcp, _| async move { Ok(domain.app.clone()) }),
        )
        .app_tools(&app(), &[ListFilesRequest::TOOL], &[UiVisibility::Model, UiVisibility::App])
        .app_tools(&app(), &[ThumbnailRequest::TOOL], &[UiVisibility::App])
        .build()
});

impl DomainServer for FilesMcp {
    type Contract = FilesContract;
    fn setup() -> &'static McpServerSetup<FilesContract> {
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

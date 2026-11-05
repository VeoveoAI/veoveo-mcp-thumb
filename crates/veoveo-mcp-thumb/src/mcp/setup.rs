//! The server's checked contract: slug, scheme, scope, documents, skills, the fixed
//! resources it lists and the templates it declares.
use std::sync::LazyLock;

use rmcp::model::{ServerCapabilities, ServerConfig};
use veoveo_hub_sdk::{
    docs::ServerDocs,
    server_contract::{McpServerContract, McpServerSetup},
    skills::ServerSkills,
};
use veoveo_types::ResourceScheme;

use crate::contract::{ThumbProvider, ThumbScope};

/// The crate documents embedded at build time and served under `thumb://docs`,
/// `thumb://docs/{doc_id}`, `thumb://contract` and the admin document routes.
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_hub_sdk::server_docs!("thumb"));

/// The skills under `skills/`, embedded at build time by `build.rs` and served at
/// `skill://thumb/…`.
pub(crate) static SERVER_SKILLS: LazyLock<ServerSkills> =
    LazyLock::new(|| veoveo_hub_sdk::server_skills!("thumb"));

pub struct ThumbContract;

pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<ThumbContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("the server's MCP contract setup"));

impl McpServerContract for ThumbContract {
    type Scope = ThumbScope;
    type Server = super::ThumbMcp;
    type Provider = ThumbProvider;

    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("thumb").expect("declared thumb scheme")
    }
    fn scopes() -> &'static [ThumbScope] {
        <ThumbScope as veoveo_types::Vocabulary>::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn skills() -> &'static ServerSkills {
        &SERVER_SKILLS
    }
    fn server_config() -> ServerConfig {
        server_config()
    }
}

fn server_config() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .build();
    if let Some(registration) = super::declarations::REGISTRATION.get() {
        veoveo_hub_sdk::registration::mcp::advertise(
            &mut capabilities,
            registration,
            &super::requirements::AUTHORIZATION,
        );
    }
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new("thumb", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(INSTRUCTIONS.to_owned());
    info
}

/// What the server is and which skill says how to use it. A server's instructions say no
/// more: its guidance is in its skills.
const INSTRUCTIONS: &str = "Makes a thumbnail of a file you can read in a Lore repository or Artifact. \
     Its skill says how: skill://thumb/make-thumbnails/SKILL.md.";

#[cfg(test)]
mod tests {
    #[test]
    fn the_instructions_name_every_skill_the_server_serves() {
        let skills = &*super::SERVER_SKILLS;
        assert_eq!(skills.skills().len(), 1);
        for skill in skills.skills() {
            assert!(super::INSTRUCTIONS.contains(&skill.uri), "{}", skill.uri);
        }
    }

    #[test]
    fn the_setup_admits() {
        std::sync::LazyLock::force(&super::SERVER_SETUP);
    }
}

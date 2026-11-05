//! The Thumb server's library. The `contract` feature holds the server's public types; the default
//! `server` feature adds the hosted server, which makes a thumbnail of a file in Lore or Artifact as
//! the person who asked.

#[cfg(feature = "contract")]
pub mod contract;

#[cfg(feature = "server")]
pub mod confine;
#[cfg(feature = "server")]
pub mod hub;
#[cfg(feature = "server")]
pub mod mcp;
#[cfg(feature = "server")]
pub mod render;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod thumbnail;

#[cfg(feature = "contract")]
use veoveo_types::ServerSlug;

/// The component name, taken from the crate name.
pub fn component() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// The server's slug, which names its roles and its URI scheme.
#[cfg(feature = "contract")]
pub fn server() -> ServerSlug {
    ServerSlug::parse("thumb").expect("the server's slug")
}

#[cfg(test)]
mod tests {
    #[test]
    fn component_is_the_crate_name() {
        assert_eq!(super::component(), "veoveo-mcp-thumb");
    }
}

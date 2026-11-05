//! The server's resource addresses under the `files` scheme. The route declaration
//! owns each shape and its typed parameters; the shared URI machinery owns encoding and
//! parsing, so no address is built or split by hand.
use std::{borrow::Cow, error::Error, fmt};

use veoveo_types::{ResourceFieldCodec, ResourceRouteError, ResourceUriError, Vocabulary};

/// A resource address that failed admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesResourceError {
    Uri(ResourceUriError),
    UnknownResource,
}

impl fmt::Display for FilesResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => error.fmt(f),
            Self::UnknownResource => f.write_str("unknown or noncanonical files resource address"),
        }
    }
}

impl Error for FilesResourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ResourceUriError> for FilesResourceError {
    fn from(error: ResourceUriError) -> Self {
        Self::Uri(error)
    }
}

/// A document the server embeds and serves under `files://docs/{doc_id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Vocabulary)]
pub enum FilesDocument {
    /// The agent work manual, `AGENTS.md`.
    Agents,
    /// The design, `DESIGN.md`.
    Design,
}

/// A resource of the server.
#[veoveo_types::resource_address(routes(FilesAddresses), schema = derived, wire, parse = borrowed, display)]
pub enum FilesResource {
    #[resource(template = "files://docs")]
    Docs,
    #[resource(template = "files://docs/{doc_id}")]
    Document(#[resource(variable = "doc_id", codec = FilesFieldCodec)] FilesDocument),
    #[resource(template = "files://contract")]
    Contract,
}

fn route_error(error: ResourceRouteError) -> FilesResourceError {
    match error {
        ResourceRouteError::Uri(error) => FilesResourceError::Uri(error),
        _ => FilesResourceError::UnknownResource,
    }
}

struct FilesFieldCodec;

impl ResourceFieldCodec<FilesDocument> for FilesFieldCodec {
    type Error = FilesResourceError;
    fn parse(value: &str) -> Result<FilesDocument, Self::Error> {
        value
            .parse()
            .map_err(|_| FilesResourceError::UnknownResource)
    }
    fn text(value: &FilesDocument) -> Cow<'_, str> {
        value.as_str().into()
    }
}

#[doc(hidden)]
pub struct FilesAddresses;
impl veoveo_types::ResourceProfile for FilesAddresses {
    type Error = FilesResourceError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, error| route_error(error),
        };
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_types::ResourceAddress;

    fn parse(uri: &str) -> Result<FilesResource, FilesResourceError> {
        FilesResource::parse(uri)
    }

    #[test]
    fn addresses_round_trip_through_their_routes() {
        for uri in ["files://docs", "files://docs/agents", "files://contract"] {
            let address = parse(uri).unwrap_or_else(|error| panic!("{uri}: {error}"));
            assert_eq!(address.to_uri().unwrap().as_str(), uri);
        }
    }

    #[test]
    fn malformed_addresses_are_refused() {
        for uri in [
            "files://docs/unknown",
            "files://other",
            "files://contract/extra",
            "media://contract",
        ] {
            assert!(parse(uri).is_err(), "{uri}");
        }
    }
}

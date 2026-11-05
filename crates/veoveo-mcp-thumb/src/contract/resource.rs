//! The server's resource addresses under the `thumb` scheme. The route declaration
//! owns each shape and its typed parameters; the shared URI machinery owns encoding and
//! parsing, so no address is built or split by hand.
use std::{borrow::Cow, error::Error, fmt};

use veoveo_types::{ResourceFieldCodec, ResourceRouteError, ResourceUriError, Vocabulary};

/// A resource address that failed admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbResourceError {
    Uri(ResourceUriError),
    UnknownResource,
}

impl fmt::Display for ThumbResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => error.fmt(f),
            Self::UnknownResource => f.write_str("unknown or noncanonical thumb resource address"),
        }
    }
}

impl Error for ThumbResourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ResourceUriError> for ThumbResourceError {
    fn from(error: ResourceUriError) -> Self {
        Self::Uri(error)
    }
}

/// A document the server embeds and serves under `thumb://docs/{doc_id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Vocabulary)]
pub enum ThumbDocument {
    /// The agent work manual, `AGENTS.md`.
    Agents,
    /// The design, `DESIGN.md`.
    Design,
}

/// A resource of the server.
#[veoveo_types::resource_address(routes(ThumbAddresses), schema = derived, wire, parse = borrowed, display)]
pub enum ThumbResource {
    #[resource(template = "thumb://docs")]
    Docs,
    #[resource(template = "thumb://docs/{doc_id}")]
    Document(#[resource(variable = "doc_id", codec = ThumbFieldCodec)] ThumbDocument),
    #[resource(template = "thumb://contract")]
    Contract,
}

fn route_error(error: ResourceRouteError) -> ThumbResourceError {
    match error {
        ResourceRouteError::Uri(error) => ThumbResourceError::Uri(error),
        _ => ThumbResourceError::UnknownResource,
    }
}

struct ThumbFieldCodec;

impl ResourceFieldCodec<ThumbDocument> for ThumbFieldCodec {
    type Error = ThumbResourceError;
    fn parse(value: &str) -> Result<ThumbDocument, Self::Error> {
        value
            .parse()
            .map_err(|_| ThumbResourceError::UnknownResource)
    }
    fn text(value: &ThumbDocument) -> Cow<'_, str> {
        value.as_str().into()
    }
}

#[doc(hidden)]
pub struct ThumbAddresses;
impl veoveo_types::ResourceProfile for ThumbAddresses {
    type Error = ThumbResourceError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, error| route_error(error),
        };
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_types::ResourceAddress;

    fn parse(uri: &str) -> Result<ThumbResource, ThumbResourceError> {
        ThumbResource::parse(uri)
    }

    #[test]
    fn addresses_round_trip_through_their_routes() {
        for uri in ["thumb://docs", "thumb://docs/agents", "thumb://contract"] {
            let address = parse(uri).unwrap_or_else(|error| panic!("{uri}: {error}"));
            assert_eq!(address.to_uri().unwrap().as_str(), uri);
        }
    }

    #[test]
    fn malformed_addresses_are_refused() {
        for uri in [
            "thumb://docs/unknown",
            "thumb://other",
            "thumb://contract/extra",
            "media://contract",
        ] {
            assert!(parse(uri).is_err(), "{uri}");
        }
    }
}

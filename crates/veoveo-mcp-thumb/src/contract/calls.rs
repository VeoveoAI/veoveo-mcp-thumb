//! The server as a provider of typed calls, available without the server runtime.
use veoveo_hub_sdk::calls::ProviderContract;
use veoveo_types::ServerSlug;

pub struct ThumbProvider;
impl ProviderContract for ThumbProvider {
    fn slug() -> ServerSlug {
        crate::server()
    }
}

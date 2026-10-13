pub mod aps;
pub mod auction;
pub mod config;
pub mod mediation;
pub mod openrtb;
pub mod render;
pub mod routes;
pub mod verification;

edgezero_core::app!("../../edgezero.toml", MocktioneerApp);

use edgezero_core::PlatformMetadata;
use edgezero_core::app::App;
use edgezero_core::error::EdgeError;

/// Build the configured Mocktioneer application.
///
/// # Errors
///
/// Returns an error if the typed application configuration cannot be built.
#[inline]
pub fn build_app() -> Result<App, EdgeError> {
    App::build::<MocktioneerApp>(PlatformMetadata::default())
}

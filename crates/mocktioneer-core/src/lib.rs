pub mod aps;
pub mod auction;
pub mod config;
pub mod mediation;
pub mod openrtb;
pub mod render;
pub mod routes;
pub mod verification;

edgezero_core::app!("../../edgezero.toml", MocktioneerApp);

use edgezero_core::app::{App, Hooks as _};

#[inline]
#[must_use]
pub fn build_app() -> App {
    MocktioneerApp::build_app()
}

pub mod aps;
pub mod auction;
pub mod mediation;
pub mod openrtb;
pub mod render;
pub mod routes;
pub mod verification;

edgezero_core::app!("../../edgezero.toml", MocktioneerApp);

use edgezero_core::app::Hooks;

pub fn build_app() -> edgezero_core::app::App {
    MocktioneerApp::build_app()
}

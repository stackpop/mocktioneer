pub mod auction;
pub mod openrtb;
pub mod render;
pub mod routes;
pub mod verification;

anyedge_core::app!("../../anyedge.toml", MocktioneerApp);

use anyedge_core::app::Hooks;

pub fn build_app() -> anyedge_core::app::App {
    MocktioneerApp::build_app()
}

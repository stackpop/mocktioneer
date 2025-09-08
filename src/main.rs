#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
use fastly::{Error, Request, Response};
#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
use anyedge_core::Logging;
#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
use mocktioneer::config::AppConfig;

#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    // Load config from embedded TOML and initialize logging via AnyEdge
    const CONFIG_TOML: &str = include_str!("../mocktioneer.toml");
    let cfg = AppConfig::from_toml_str(CONFIG_TOML).expect("valid config");
    anyedge_fastly::register_logger(cfg.logging.endpoint.clone(), cfg.logging.level, true);
    Logging::init_logging();

    let app = mocktioneer::build_app();
    Ok(anyedge_fastly::handle(&app, req))
}

#[cfg(not(all(target_arch = "wasm32", target_os = "wasi")))]
fn main() {
    // Native build stub
    eprintln!("mocktioneer: target wasm32-wasi to run on Fastly.");
}

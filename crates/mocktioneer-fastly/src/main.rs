// Note: even when targeting the wasm32-wasip1 triple, Rust's cfg `target_os` remains "wasi".
#[cfg(all(target_arch = "wasm32"))]
use fastly::{Error, Request, Response};
#[cfg(all(target_arch = "wasm32"))]
use mocktioneer_core::config::AppConfig;

#[cfg(all(target_arch = "wasm32"))]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    // Load config from embedded TOML and initialize logging via AnyEdge
    const CONFIG_TOML: &str = include_str!("../../../mocktioneer.toml");
    let cfg = AppConfig::from_toml_str(CONFIG_TOML).expect("valid config");
    // Fastly target: initialize logger directly (formatted)
    anyedge_fastly::init_logger(&cfg.logging.endpoint, cfg.logging.level, true)
        .expect("init fastly logger");

    let app = mocktioneer_core::build_app();
    Ok(anyedge_fastly::handle(&app, req))
}

#[cfg(not(all(target_arch = "wasm32")))]
fn main() {
    eprintln!("mocktioneer-fastly: target wasm32-wasip1 to run on Fastly.");
}

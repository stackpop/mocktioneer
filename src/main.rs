#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
use fastly::{Error, Request, Response};
#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
use mocktioneer::config::AppConfig;

#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    // Load config from embedded TOML and initialize logging via AnyEdge
    const CONFIG_TOML: &str = include_str!("../mocktioneer.toml");
    let cfg = AppConfig::from_toml_str(CONFIG_TOML).expect("valid config");
    // Fastly target: initialize logger directly (formatted)
    anyedge_fastly::init_logger(
        &cfg.logging.endpoint,
        cfg.logging.level,
        true,
    ).expect("init fastly logger");

    let app = mocktioneer::build_app();
    Ok(anyedge_fastly::handle(&app, req))
}

#[cfg(not(all(target_arch = "wasm32", target_os = "wasi")))]
fn main() {
    eprintln!("mocktioneer: target wasm32-wasi to run on Fastly.");
}

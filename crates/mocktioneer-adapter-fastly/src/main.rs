// Note: even when targeting the wasm32-wasip1 triple, Rust's cfg `target_os` remains "wasi".
#[cfg(target_arch = "wasm32")]
use fastly::{Error, Request, Response};
#[cfg(target_arch = "wasm32")]
use mocktioneer_core::config::{AppConfig, LoggingProvider};

#[cfg(target_arch = "wasm32")]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    const CONFIG_TOML: &str = include_str!("../../../mocktioneer.toml");
    let cfg = AppConfig::from_toml_str(CONFIG_TOML).expect("valid config");
    init_logger(&cfg).expect("init logger");

    let app = mocktioneer_core::build_app();
    anyedge_adapter_fastly::dispatch(&app, req)
}

#[cfg(target_arch = "wasm32")]
fn init_logger(cfg: &AppConfig) -> Result<(), log::SetLoggerError> {
    match cfg.logging.provider {
        LoggingProvider::Fastly => {
            anyedge_adapter_fastly::init_logger(&cfg.logging.endpoint, cfg.logging.level, false)?;
            Ok(())
        }
        LoggingProvider::Stdout => simple_logger::SimpleLogger::new()
            .with_level(cfg.logging.level)
            .init(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("mocktioneer-adapter-fastly: target wasm32-wasip1 to run on Fastly.");
}

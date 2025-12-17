#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
use edgezero_adapter_fastly::run_app;
#[cfg(target_arch = "wasm32")]
use fastly::{Error, Request, Response};
#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;

#[cfg(target_arch = "wasm32")]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    run_app::<MocktioneerApp>(include_str!("../../../edgezero.toml"), req)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("mocktioneer-adapter-fastly: target wasm32-wasip1 to run on Fastly.");
}

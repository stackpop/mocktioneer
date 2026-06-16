#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "host build only carries the wasm-target stub; real entry point compiles only under wasm32"
    )
)]

#[cfg(target_arch = "wasm32")]
use fastly::{Error, Request, Response};
#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    edgezero_adapter_fastly::run_app::<MocktioneerApp>(req)
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(
    clippy::print_stderr,
    reason = "host-side stub that exists solely to remind the operator to target wasm32"
)]
fn main() {
    eprintln!("mocktioneer-adapter-fastly: target wasm32-wasip1 to run on Fastly.");
}

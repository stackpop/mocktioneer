#![cfg_attr(target_arch = "wasm32", no_main)]

#[cfg(target_arch = "wasm32")]
use mocktioneer_core::MocktioneerApp;
#[cfg(target_arch = "wasm32")]
use worker::*;

#[cfg(target_arch = "wasm32")]
#[event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    anyedge_adapter_cloudflare::run_app::<MocktioneerApp>(req, env, ctx).await
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("mocktioneer-adapter-cloudflare: build with --target wasm32-unknown-unknown to run.");
}

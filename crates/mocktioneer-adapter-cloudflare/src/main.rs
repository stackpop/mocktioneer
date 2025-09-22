#[cfg(target_arch = "wasm32")]
use mocktioneer_core::build_app;
#[cfg(target_arch = "wasm32")]
use worker::*;

#[cfg(target_arch = "wasm32")]
#[event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let app = build_app();
    anyedge_adapter_cloudflare::dispatch(&app, req, env, ctx).await
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("mocktioneer-adapter-cloudflare: build with --target wasm32-unknown-unknown to run.");
}

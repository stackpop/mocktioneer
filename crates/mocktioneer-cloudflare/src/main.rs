use mocktioneer_core::build_app;
use worker::*;

#[event(fetch)]
pub async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let app = build_app();
    anyedge_cloudflare::handle(&app, req, env, ctx).await
}

#[cfg(not(all(target_arch = "wasm32")))]
fn main() {
    eprintln!("mocktioneer-fastly: target wasm32-wasip1 to run on Fastly.");
}

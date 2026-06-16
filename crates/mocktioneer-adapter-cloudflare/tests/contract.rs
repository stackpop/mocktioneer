#![cfg(all(feature = "cloudflare", target_arch = "wasm32"))]
#![expect(
    deprecated,
    reason = "exercise the low-level dispatch path while it remains public"
)]

use edgezero_adapter_cloudflare::request::dispatch;
use mocktioneer_core::build_app;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use worker::wasm_bindgen::JsCast as _;
use worker::{Context, Env, Method as CfMethod, Request as CfRequest, RequestInit};

// `run_in_browser` selects the wasm-bindgen browser harness. In CI this runs
// headless in Firefox (via the geckodriver that ships on the ubuntu-latest
// runners); locally it picks up whichever browser driver is on PATH.
// `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner` is
// what wires the runner into `cargo test`; the harness mode is declared here.
wasm_bindgen_test_configure!(run_in_browser);

fn cf_request(method: CfMethod, path: &str) -> CfRequest {
    let mut init = RequestInit::new();
    init.with_method(method);

    let headers = worker::Headers::new();
    headers.set("host", "test.local").expect("host header");
    init.with_headers(headers);

    let url = format!("https://test.local{path}");
    CfRequest::new_with_init(&url, &init).expect("cf request")
}

fn test_env_ctx() -> (Env, Context) {
    let env = worker::js_sys::Object::new().unchecked_into::<Env>();
    let js_context = worker::js_sys::Object::new().unchecked_into::<worker::worker_sys::Context>();
    (env, Context::new(js_context))
}

#[wasm_bindgen_test]
async fn root_dispatches_through_cloudflare_adapter() {
    let app = build_app();
    let req = cf_request(CfMethod::Get, "/");
    let (env, ctx) = test_env_ctx();

    let mut response = dispatch(&app, req, env, ctx).await.expect("cf response");

    assert_eq!(response.status_code(), 200);
    let body = response.bytes().await.expect("body bytes");
    let body_str = std::str::from_utf8(&body).expect("utf8 body");
    assert!(
        body_str.starts_with("<!doctype html>") || body_str.starts_with("<!DOCTYPE html>"),
        "expected HTML body, got prefix: {:?}",
        &body_str.chars().take(40).collect::<String>(),
    );
}

#[wasm_bindgen_test]
async fn pixel_returns_gif_through_cloudflare_adapter() {
    let app = build_app();
    let req = cf_request(CfMethod::Get, "/pixel?pid=cloudflare-contract");
    let (env, ctx) = test_env_ctx();

    let response = dispatch(&app, req, env, ctx).await.expect("cf response");

    assert_eq!(response.status_code(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type lookup")
        .expect("content-type header");
    assert_eq!(content_type, "image/gif");
}

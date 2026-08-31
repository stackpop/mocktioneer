//! Spin adapter contract tests.
//!
//! Spin's `Request` is a WASI handle that can only be constructed by
//! the Spin runtime, so we cannot exercise the full `dispatch(req)` path the
//! way the Fastly and Cloudflare contract tests do. Instead, these tests
//! prove that `MocktioneerApp::build_app()` and `router().oneshot(...)` work
//! end-to-end under `wasm32-wasip2` via the `wasmtime` runner — the same
//! integration surface the Spin adapter's `run_app` calls into.

#![cfg(all(feature = "spin", target_arch = "wasm32"))]

use edgezero_core::body::Body;
use edgezero_core::http::{Method, Request, Response, StatusCode, header, request_builder};
use futures::executor::block_on;
use mocktioneer_core::build_app;

fn make_request(method: Method, uri: &str) -> Request {
    request_builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "mocktioneer.test")
        .body(Body::empty())
        .expect("request")
}

fn dispatch(request: Request) -> Response {
    let app = build_app();
    block_on(app.router().oneshot(request)).expect("router result")
}

#[test]
fn root_dispatches_through_spin_runtime() {
    let response = dispatch(make_request(Method::GET, "/"));
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert!(content_type.starts_with("text/html"));
}

#[test]
fn pixel_returns_gif_through_spin_runtime() {
    let response = dispatch(make_request(Method::GET, "/pixel?pid=spin-contract"));
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert_eq!(content_type, "image/gif");
}

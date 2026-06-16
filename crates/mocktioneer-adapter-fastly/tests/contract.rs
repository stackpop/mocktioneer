#![cfg(all(feature = "fastly", target_arch = "wasm32"))]

use edgezero_adapter_fastly::request::FastlyService;
use fastly::http::{Method as FastlyMethod, StatusCode as FastlyStatus};
use fastly::Request as FastlyRequest;
use mocktioneer_core::build_app;

fn fastly_request(method: FastlyMethod, path: &str) -> FastlyRequest {
    let mut req = FastlyRequest::new(method, format!("http://test.local{path}"));
    req.set_header("host", "test.local");
    req
}

#[test]
fn root_dispatches_through_fastly_adapter() {
    let app = build_app();
    let req = fastly_request(FastlyMethod::GET, "/");

    let mut response = FastlyService::new(&app).dispatch(req).expect("fastly response");

    assert_eq!(response.get_status(), FastlyStatus::OK);
    let body = response.take_body_bytes();
    let body_str = std::str::from_utf8(&body).expect("utf8 body");
    assert!(
        body_str.starts_with("<!doctype html>") || body_str.starts_with("<!DOCTYPE html>"),
        "expected HTML body, got prefix: {:?}",
        &body_str.chars().take(40).collect::<String>(),
    );
}

#[test]
fn pixel_returns_gif_through_fastly_adapter() {
    let app = build_app();
    let req = fastly_request(FastlyMethod::GET, "/pixel?pid=fastly-contract");

    let response = FastlyService::new(&app).dispatch(req).expect("fastly response");

    assert_eq!(response.get_status(), FastlyStatus::OK);
    let content_type = response
        .get_header("content-type")
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert_eq!(content_type, "image/gif");
}

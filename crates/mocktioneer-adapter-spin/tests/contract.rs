//! Spin adapter contract tests.
//!
//! Exercise normalized-head validation, admission, dispatch, and response-egress
//! framing/completion with Spin platform metadata under `wasm32-wasip2` via
//! Wasmtime. Spin's runtime-owned `Request` handle cannot be constructed here;
//! these tests do not exercise its request conversion or provider response writer.

#![cfg(all(feature = "spin", target_arch = "wasm32"))]
#![cfg(test)]

use edgezero_adapter_spin::SPIN_PLATFORM;
use edgezero_core::app::App;
use edgezero_core::body::Body;
use edgezero_core::http::{Method, Request, Response, StatusCode, header, request_builder};
use edgezero_core::response_egress_framing::ResponseEgressFramingMode;
use edgezero_core::{
    IngressBeginOutcome, IngressFraming, IngressHeadAccounting, IngressHeadParts, MonotonicClock,
};
use futures::executor::block_on;
use futures::stream::poll_fn;
use mocktioneer_core::MocktioneerApp;
use std::str;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

fn app() -> App {
    App::build::<MocktioneerApp>(SPIN_PLATFORM).expect("build app")
}

fn make_request(method: Method, uri: &str) -> Request {
    request_builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "mocktioneer.test")
        .body(Body::empty())
        .expect("request")
}

fn dispatch(app: &App, request: Request) -> Response {
    let start = app.monotonic_clock().now();
    let head = IngressHeadParts::from_request(
        &request,
        IngressHeadAccounting::HostManaged,
        IngressFraming::HostManaged,
    );
    head.validate_normalized(app.ingress_head_limits())
        .expect("valid normalized head");
    let IngressBeginOutcome::Admitted(ingress) =
        app.begin_ingress(head, start).expect("begin ingress")
    else {
        panic!("expected admitted request");
    };
    let envelope = block_on(app.dispatch_admitted(ingress, request));
    let Ok((prepared, _, mut attempt, clock)) =
        envelope.begin(ResponseEgressFramingMode::ExplicitBufferedLength)
    else {
        panic!("prepare response egress");
    };
    assert!(attempt.begin_writing(), "response write must start");
    assert!(
        attempt.complete(clock.now()),
        "response egress must complete"
    );
    prepared.into_response()
}

#[test]
fn root_dispatches_through_spin_runtime() {
    let response = dispatch(&app(), make_request(Method::GET, "/"));
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
    let response = dispatch(
        &app(),
        make_request(Method::GET, "/pixel?pid=spin-contract"),
    );
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert_eq!(content_type, "image/gif");
}

#[test]
fn spin_platform_preserves_bid_and_config_read_budgets() {
    let application = app();
    assert_eq!(application.platform(), SPIN_PLATFORM);
    assert_eq!(
        application.config_extraction_limits().timeout,
        Duration::from_secs(2)
    );
    let start = application.monotonic_clock().now();
    for (method, path, seconds) in [
        (Method::POST, "/openrtb2/auction", 10),
        (Method::POST, "/e/dtb/bid", 10),
        (Method::POST, "/adserver/mediate", 10),
        (Method::GET, "/", 30),
        (Method::OPTIONS, "/openrtb2/auction", 30),
    ] {
        let request = make_request(method, path);
        let head = IngressHeadParts::from_request(
            &request,
            IngressHeadAccounting::HostManaged,
            IngressFraming::HostManaged,
        );
        head.validate_normalized(application.ingress_head_limits())
            .expect("valid normalized head");
        let IngressBeginOutcome::Admitted(prepared) = application
            .begin_ingress(head, start)
            .expect("begin ingress")
        else {
            panic!("expected admitted request");
        };
        assert_eq!(
            prepared
                .read_deadline()
                .instant()
                .checked_duration_since(start),
            Some(Duration::from_secs(seconds))
        );
    }
}

#[test]
fn mediation_body_expiry_returns_finalized_timeout_with_cors() {
    let mut application = app();
    let start = application.monotonic_clock().now();
    let now = Arc::new(Mutex::new(start));
    let observed = Arc::clone(&now);
    application.set_monotonic_clock(MonotonicClock::new(move || {
        *observed.lock().expect("clock lock")
    }));
    let mut chunk = Some(b"{}".to_vec());
    let body = Body::from_stream(poll_fn(move |_| {
        *now.lock().expect("clock lock") = start
            .checked_add(Duration::from_secs(10))
            .expect("deadline");
        Poll::Ready(chunk.take().map(|bytes| Ok(bytes.into())))
    }));
    let mut request = make_request(Method::POST, "/adserver/mediate");
    *request.body_mut() = body;
    let response = dispatch(&application, request);
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    for (name, value) in [
        ("access-control-allow-origin", "*"),
        ("access-control-allow-methods", "GET, POST, OPTIONS"),
        ("access-control-allow-headers", "*, content-type"),
    ] {
        assert_eq!(response.headers().get(name).expect("CORS header"), value);
    }
    let bytes = response
        .into_body()
        .into_bytes()
        .expect("buffered error body");
    let error = str::from_utf8(&bytes).expect("UTF-8 error body");
    assert!(error.contains(r#""kind":"request_timeout""#));
    assert!(error.contains(r#""message":"inbound body read deadline exceeded""#));
    assert!(error.contains(r#""status":408"#));
}

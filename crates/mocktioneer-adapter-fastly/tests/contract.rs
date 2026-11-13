#![cfg(all(feature = "fastly", target_arch = "wasm32"))]

use async_trait::async_trait;
use edgezero_adapter_fastly::request::FastlyService;
use edgezero_core::blob_envelope::BlobEnvelope;
use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
use edgezero_core::http::{Response, StatusCode};
use edgezero_core::response_egress_framing::ResponseEgressFramingMode;
use fastly::Request as FastlyRequest;
use fastly::http::Method as FastlyMethod;
use futures::executor::block_on;
use mocktioneer_core::MocktioneerApp;
use std::collections::HashMap;
use std::sync::Arc;

/// In-memory `ConfigStore` so the auction path can be seeded in-process, with no
/// Viceroy config-store fixture. Mirrors the core-crate test double.
struct MapConfigStore(HashMap<String, String>);

#[async_trait(?Send)]
impl ConfigStore for MapConfigStore {
    async fn get(
        &self,
        key: &str,
    ) -> Result<Option<edgezero_core::config_store::ConfigValue>, ConfigStoreError> {
        Ok(self.0.get(key).cloned().map(Into::into))
    }

    async fn get_bounded(
        &self,
        key: &str,
        clock: &edgezero_core::MonotonicClock,
        deadline: edgezero_core::Deadline,
        max_backend_bytes: u64,
        max_value_bytes: u64,
    ) -> Result<
        edgezero_core::config_store::BoundedStoreRead<edgezero_core::config_store::ConfigValue>,
        ConfigStoreError,
    > {
        if deadline.is_expired_at(clock.now()) {
            return Err(ConfigStoreError::DeadlineExceeded);
        }
        edgezero_core::config_store::finish_bounded_config_read(
            Ok(self.0.get(key).cloned()),
            clock,
            deadline,
            max_backend_bytes,
            max_value_bytes,
        )
    }
}

/// A config handle holding the typed-config blob for `{ "bid_cpm": <cpm> }`.
/// An injected handle is bound under default_key `"default"` by the Fastly
/// service builder (`synthesise_store_registries`), so the blob lives there.
fn seeded_config_handle(bid_cpm: f64) -> ConfigStoreHandle {
    let data = serde_json::json!({ "bid_cpm": bid_cpm });
    let blob = serde_json::to_string(&BlobEnvelope::new(data, "2026-01-01T00:00:00Z".to_owned()))
        .expect("serialize envelope");
    let store = MapConfigStore([("default".to_owned(), blob)].into_iter().collect());
    ConfigStoreHandle::new(Arc::new(store))
}

fn fastly_request(method: FastlyMethod, path: &str) -> FastlyRequest {
    let mut req = FastlyRequest::new(method, format!("http://test.local{path}"));
    req.set_header("host", "test.local");
    req
}

#[test]
fn root_dispatches_through_fastly_adapter() {
    let app =
        edgezero_core::app::App::build::<MocktioneerApp>(edgezero_adapter_fastly::FASTLY_PLATFORM)
            .expect("build app");
    let req = fastly_request(FastlyMethod::GET, "/");

    let response = FastlyService::new(&app)
        .capture_egress_for_test(req)
        .expect("fastly response");
    let response = complete_envelope(response);

    assert_eq!(response.status(), StatusCode::OK);
    let body =
        block_on(response.into_body().into_bytes_bounded(2 * 1024 * 1024)).expect("response body");
    let body_str = std::str::from_utf8(&body).expect("utf8 body");
    assert!(
        body_str.starts_with("<!doctype html>") || body_str.starts_with("<!DOCTYPE html>"),
        "expected HTML body, got prefix: {:?}",
        &body_str.chars().take(40).collect::<String>(),
    );
}

#[test]
fn pixel_returns_gif_through_fastly_adapter() {
    let app =
        edgezero_core::app::App::build::<MocktioneerApp>(edgezero_adapter_fastly::FASTLY_PLATFORM)
            .expect("build app");
    let req = fastly_request(FastlyMethod::GET, "/pixel?pid=fastly-contract");

    let response = FastlyService::new(&app)
        .capture_egress_for_test(req)
        .expect("fastly response");
    let response = complete_envelope(response);

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert_eq!(content_type, "image/gif");
}

#[test]
fn auction_uses_seeded_cpm_through_fastly_adapter() {
    let app =
        edgezero_core::app::App::build::<MocktioneerApp>(edgezero_adapter_fastly::FASTLY_PLATFORM)
            .expect("build app");
    let body = serde_json::json!({
        "id": "rc",
        "imp": [{ "id": "1", "banner": { "w": 300_i32, "h": 250_i32 } }]
    })
    .to_string();
    let req = fastly_request(FastlyMethod::POST, "/openrtb2/auction")
        .with_header("content-type", "application/json")
        .with_body(body);

    let response = FastlyService::new(&app)
        .with_config_handle(seeded_config_handle(0.35_f64))
        .capture_egress_for_test(req)
        .expect("fastly response");
    let response = complete_envelope(response);

    // Full path through the adapter: request translation -> config-store bind ->
    // `AppConfig` extractor -> auction handler -> response translation.
    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value = serde_json::from_slice(
        &block_on(response.into_body().into_bytes_bounded(2 * 1024 * 1024)).expect("response body"),
    )
    .expect("json body");
    let price = payload["seatbid"][0]["bid"][0]["price"]
        .as_f64()
        .expect("bid price");
    assert!(
        (price - 0.35_f64).abs() < f64::EPSILON,
        "expected seeded cpm 0.35, got {price}",
    );
}

fn complete_envelope(envelope: edgezero_core::ResponseEgressEnvelope) -> Response {
    let Ok((prepared, _, mut attempt, clock)) =
        envelope.begin(ResponseEgressFramingMode::HostAutomatic)
    else {
        panic!("begin test egress");
    };
    assert!(attempt.begin_writing());
    assert!(attempt.complete(clock.now()));
    prepared.into_response()
}

#[test]
fn synthetic_empty_statuses_have_zero_length_through_fastly_prepared_egress() {
    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let router = edgezero_core::router::RouterBuilder::new()
            .get(
                "/empty",
                move |_ctx: edgezero_core::context::RequestContext| async move {
                    Ok::<_, edgezero_core::EdgeError>(
                        edgezero_core::http::response_builder()
                            .status(status)
                            .header("content-length", "0")
                            .body(edgezero_core::body::Body::empty())
                            .expect("empty response"),
                    )
                },
            )
            .build();
        let app = edgezero_core::app::App::new(router);
        let envelope = FastlyService::new(&app)
            .capture_egress_for_test(fastly_request(FastlyMethod::GET, "/empty"))
            .expect("capture egress");
        let Ok((prepared, _, mut attempt, clock)) =
            envelope.begin(ResponseEgressFramingMode::HostAutomatic)
        else {
            panic!("prepare empty response");
        };
        assert_eq!(prepared.declared_length(), Some(0));
        assert!(prepared.transmits_body());
        assert_eq!(prepared.into_response().status(), status);
        assert!(attempt.begin_writing());
        assert!(attempt.complete(clock.now()));
    }
}

#[test]
fn mocktioneer_empty_handlers_preserve_status_specific_framing() {
    let app =
        edgezero_core::app::App::build::<MocktioneerApp>(edgezero_adapter_fastly::FASTLY_PLATFORM)
            .expect("build Mocktioneer app");
    for (method, path, status, content_length) in [
        (
            FastlyMethod::GET,
            "/sync/start?ts_domain=bad%2Fhost",
            StatusCode::BAD_REQUEST,
            Some("0"),
        ),
        (
            FastlyMethod::GET,
            "/sync/start?ts_domain=ts.publisher.com",
            StatusCode::FOUND,
            Some("0"),
        ),
        (
            FastlyMethod::OPTIONS,
            "/openrtb2/auction",
            StatusCode::NO_CONTENT,
            None,
        ),
    ] {
        let response = complete_envelope(
            FastlyService::new(&app)
                .capture_egress_for_test(fastly_request(method, path))
                .expect("Mocktioneer egress"),
        );
        assert_eq!(response.status(), status);
        assert_eq!(
            response
                .headers()
                .get("content-length")
                .map(|value| value.to_str().unwrap()),
            content_length
        );
        if status == StatusCode::FOUND {
            assert!(
                response
                    .headers()
                    .get("location")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("https://ts.publisher.com/sync?")
            );
        }
        assert!(
            block_on(response.into_body().into_bytes_bounded(1))
                .unwrap()
                .is_empty()
        );
    }
}

#![cfg(all(feature = "fastly", target_arch = "wasm32"))]

use async_trait::async_trait;
use edgezero_adapter_fastly::request::FastlyService;
use edgezero_core::blob_envelope::BlobEnvelope;
use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
use fastly::http::{Method as FastlyMethod, StatusCode as FastlyStatus};
use fastly::Request as FastlyRequest;
use mocktioneer_core::build_app;
use std::collections::HashMap;
use std::sync::Arc;

/// In-memory `ConfigStore` so the auction path can be seeded in-process, with no
/// Viceroy config-store fixture. Mirrors the core-crate test double.
struct MapConfigStore(HashMap<String, String>);

#[async_trait(?Send)]
impl ConfigStore for MapConfigStore {
    async fn get(&self, key: &str) -> Result<Option<String>, ConfigStoreError> {
        Ok(self.0.get(key).cloned())
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
    let app = build_app();
    let req = fastly_request(FastlyMethod::GET, "/");

    let mut response = FastlyService::new(&app)
        .dispatch(req)
        .expect("fastly response");

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

    let response = FastlyService::new(&app)
        .dispatch(req)
        .expect("fastly response");

    assert_eq!(response.get_status(), FastlyStatus::OK);
    let content_type = response
        .get_header("content-type")
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert_eq!(content_type, "image/gif");
}

#[test]
fn auction_uses_seeded_cpm_through_fastly_adapter() {
    let app = build_app();
    let body = serde_json::json!({
        "id": "rc",
        "imp": [{ "id": "1", "banner": { "w": 300_i32, "h": 250_i32 } }]
    })
    .to_string();
    let req = fastly_request(FastlyMethod::POST, "/openrtb2/auction")
        .with_header("content-type", "application/json")
        .with_body(body);

    let mut response = FastlyService::new(&app)
        .with_config_handle(seeded_config_handle(0.35_f64))
        .dispatch(req)
        .expect("fastly response");

    // Full path through the adapter: request translation -> config-store bind ->
    // `AppConfig` extractor -> auction handler -> response translation.
    assert_eq!(response.get_status(), FastlyStatus::OK);
    let payload: serde_json::Value =
        serde_json::from_slice(&response.take_body_bytes()).expect("json body");
    let price = payload["seatbid"][0]["bid"][0]["price"]
        .as_f64()
        .expect("bid price");
    assert!(
        (price - 0.35_f64).abs() < f64::EPSILON,
        "expected seeded cpm 0.35, got {price}",
    );
}

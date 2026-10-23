#![cfg(all(feature = "cloudflare", target_arch = "wasm32"))]

use async_trait::async_trait;
use edgezero_adapter_cloudflare::request::CloudflareService;
use edgezero_core::blob_envelope::BlobEnvelope;
use edgezero_core::body::Body;
use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
use edgezero_core::context::RequestContext;
use edgezero_core::http::{HeaderMap, Method, StatusCode, request_builder};
use edgezero_core::outbound::{
    ContentTypeProvenance, HttpClient, OutboundBatch, OutboundHttpClient, OutboundRequest,
    OutboundResponse,
};
use edgezero_core::params::PathParams;
use edgezero_core::{Deadline, EdgeError, MonotonicClock};
use mocktioneer_core::MocktioneerApp;
use mocktioneer_core::verification::verify_request_id_signature;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use worker::js_sys::Date;
use worker::wasm_bindgen::{JsCast as _, JsValue};
use worker::{Context, Env, Method as CfMethod, Request as CfRequest, RequestInit};

/// In-memory `ConfigStore` so the auction path can be seeded in-process, with no
/// miniflare/workerd KV fixture. Mirrors the core-crate test double.
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

struct JwksClient {
    calls: Arc<AtomicUsize>,
    reply: Vec<u8>,
}

#[async_trait(?Send)]
impl OutboundHttpClient for JwksClient {
    async fn send(&self, request: OutboundRequest) -> Result<OutboundResponse, EdgeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let parts = request.into_parts();
        assert_eq!(parts.method, Method::GET, "JWKS uses GET");
        assert_eq!(
            parts.uri.to_string(),
            "https://jwks-cache-contract.mocktioneer.example/.well-known/trusted-server.json",
            "JWKS uses the canonical publisher host"
        );
        Ok(OutboundResponse::new(
            parts.method,
            StatusCode::OK,
            HeaderMap::new(),
            Body::from_bytes(self.reply.clone()),
            MonotonicClock::default(),
            ContentTypeProvenance::Unknown,
        ))
    }

    fn start_batch_until(
        &self,
        requests: Vec<OutboundRequest>,
        _cutoff: Deadline,
    ) -> OutboundBatch {
        OutboundBatch::cutoff(requests.len())
    }
}

/// A config handle holding the typed-config blob for `{ "bid_cpm": <cpm> }`.
/// An injected handle is bound under default_key `"default"` by the Cloudflare
/// service builder (`synthesise_store_registries`), so the blob lives there.
fn seeded_config_handle(bid_cpm: f64) -> ConfigStoreHandle {
    let data = serde_json::json!({ "bid_cpm": bid_cpm });
    let blob = serde_json::to_string(&BlobEnvelope::new(data, "2026-01-01T00:00:00Z".to_owned()))
        .expect("serialize envelope");
    let store = MapConfigStore([("default".to_owned(), blob)].into_iter().collect());
    ConfigStoreHandle::new(Arc::new(store))
}

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
    let global = worker::js_sys::global();
    let install = worker::js_sys::Function::new_no_args(
        "globalThis.FixedLengthStream ??= class extends TransformStream { constructor(_length) { super(); } };",
    );
    install
        .call0(&global)
        .expect("install browser-only FixedLengthStream");
    let context = worker::js_sys::Object::new();
    worker::js_sys::Reflect::set(
        &context,
        &JsValue::from_str("waitUntil"),
        worker::js_sys::Function::new_with_args("_promise", "return undefined;").as_ref(),
    )
    .expect("install test waitUntil");
    let js_context = context.unchecked_into::<worker::worker_sys::Context>();
    (env, Context::new(js_context))
}

#[wasm_bindgen_test]
async fn root_dispatches_through_cloudflare_adapter() {
    let app = edgezero_core::app::App::build::<MocktioneerApp>(
        edgezero_adapter_cloudflare::CLOUDFLARE_PLATFORM,
    )
    .expect("build app");
    let req = cf_request(CfMethod::Get, "/");
    let (env, ctx) = test_env_ctx();

    let mut response = CloudflareService::new(&app)
        .dispatch(req, env, ctx)
        .await
        .expect("cf response");

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
    let app = edgezero_core::app::App::build::<MocktioneerApp>(
        edgezero_adapter_cloudflare::CLOUDFLARE_PLATFORM,
    )
    .expect("build app");
    let req = cf_request(CfMethod::Get, "/pixel?pid=cloudflare-contract");
    let (env, ctx) = test_env_ctx();

    let response = CloudflareService::new(&app)
        .dispatch(req, env, ctx)
        .await
        .expect("cf response");

    assert_eq!(response.status_code(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type lookup")
        .expect("content-type header");
    assert_eq!(content_type, "image/gif");
}

fn cf_post_json(path: &str, body: &str) -> CfRequest {
    let mut init = RequestInit::new();
    init.with_method(CfMethod::Post);

    let headers = worker::Headers::new();
    headers.set("host", "test.local").expect("host header");
    headers
        .set("content-type", "application/json")
        .expect("content-type header");
    init.with_headers(headers);
    init.with_body(Some(JsValue::from_str(body)));

    let url = format!("https://test.local{path}");
    CfRequest::new_with_init(&url, &init).expect("cf request")
}

#[wasm_bindgen_test]
async fn auction_uses_seeded_cpm_through_cloudflare_adapter() {
    let app = edgezero_core::app::App::build::<MocktioneerApp>(
        edgezero_adapter_cloudflare::CLOUDFLARE_PLATFORM,
    )
    .expect("build app");
    let body = serde_json::json!({
        "id": "rc",
        "imp": [{ "id": "1", "banner": { "w": 300_i32, "h": 250_i32 } }]
    })
    .to_string();
    let req = cf_post_json("/openrtb2/auction", &body);
    let (env, ctx) = test_env_ctx();

    // Full path through the adapter: request translation -> config-store bind ->
    // `AppConfig` extractor -> auction handler -> response translation.
    let mut response = CloudflareService::new(&app)
        .with_config_handle(seeded_config_handle(0.35_f64))
        .dispatch(req, env, ctx)
        .await
        .expect("cf response");

    assert_eq!(response.status_code(), 200);
    let payload: serde_json::Value =
        serde_json::from_slice(&response.bytes().await.expect("body bytes")).expect("json body");
    let price = payload["seatbid"][0]["bid"][0]["price"]
        .as_f64()
        .expect("bid price");
    assert!(
        (price - 0.35_f64).abs() < f64::EPSILON,
        "expected seeded cpm 0.35, got {price}",
    );
}

#[wasm_bindgen_test]
async fn empty_statuses_have_zero_length_through_cloudflare_egress() {
    for status in [400_u16, 401, 403, 302, 500] {
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
        let (env, ctx) = test_env_ctx();
        let mut response = CloudflareService::new(&app)
            .dispatch(cf_request(CfMethod::Get, "/empty"), env, ctx)
            .await
            .expect("cf empty response");
        assert_eq!(response.status_code(), status);
        assert_eq!(
            response.headers().get("content-length").expect("header"),
            Some("0".to_owned())
        );
        assert!(response.bytes().await.expect("empty bytes").is_empty());
    }

    let app = edgezero_core::app::App::build::<MocktioneerApp>(
        edgezero_adapter_cloudflare::CLOUDFLARE_PLATFORM,
    )
    .expect("build Mocktioneer app");
    for (path, status) in [
        ("/sync/start?ts_domain=bad%2Fhost", 400_u16),
        ("/sync/start?ts_domain=ts.publisher.com", 302_u16),
    ] {
        let (env, ctx) = test_env_ctx();
        let mut response = CloudflareService::new(&app)
            .dispatch(cf_request(CfMethod::Get, path), env, ctx)
            .await
            .expect("Mocktioneer response");
        assert_eq!(response.status_code(), status);
        assert_eq!(
            response.headers().get("content-length").expect("header"),
            Some("0".to_owned())
        );
        if status == 302 {
            assert!(
                response
                    .headers()
                    .get("location")
                    .unwrap()
                    .unwrap()
                    .starts_with("https://ts.publisher.com/sync?")
            );
        }
        assert!(
            response
                .bytes()
                .await
                .expect("empty Mocktioneer bytes")
                .is_empty()
        );
    }
}

#[wasm_bindgen_test]
#[expect(
    clippy::expect_used,
    reason = "browser signature fixture fails the wasm test on setup or verification errors"
)]
async fn wasm_jwks_fetch_then_cache_hit_verifies_signature() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signer as _, SigningKey};
    let domain = "jwks-cache-contract.mocktioneer.example";
    let request_id = "wasm-cache-request";
    let key_id = "wasm-cache-key";
    let timestamp = Date::now()
        .floor()
        .to_string()
        .parse::<u64>()
        .expect("fresh epoch milliseconds");
    let signing_key = SigningKey::from_bytes(&[73_u8; 32]);
    // This exact field order is the Trusted Server v1.1 canonical payload.
    let payload = format!(
        r#"{{"version":"1.1","kid":"{key_id}","host":"{domain}","scheme":"https","id":"{request_id}","ts":{timestamp}}}"#
    );
    let signature = URL_SAFE_NO_PAD.encode(signing_key.sign(payload.as_bytes()).to_bytes());
    let ext = serde_json::json!({"trusted_server": {
        "version": "1.1", "kid": key_id, "request_host": domain,
        "request_scheme": "https", "ts": timestamp, "signature": signature
    }});
    let reply = serde_json::to_vec(&serde_json::json!({"jwks": {"keys": [{
        "kid": key_id, "x": URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes())
    }]}}))
    .expect("JWKS reply");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut request = request_builder()
        .uri("/openrtb2/auction")
        .body(Body::empty())
        .expect("request");
    request
        .extensions_mut()
        .insert(HttpClient::with_client(JwksClient {
            reply,
            calls: Arc::clone(&calls),
        }));
    let ctx = RequestContext::new(request, PathParams::new(HashMap::new()));
    for _ in 0_u8..2 {
        assert_eq!(
            verify_request_id_signature(&ctx, request_id, Some(&ext), domain)
                .await
                .expect("verify signed request"),
            key_id,
            "fetched and cached JWKS both verify the real signature"
        );
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "second verification hits the cache"
    );
}

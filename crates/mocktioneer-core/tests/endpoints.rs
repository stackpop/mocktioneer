#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use edgezero_core::app::App;
    use edgezero_core::blob_envelope::BlobEnvelope;
    use edgezero_core::body::Body;
    use edgezero_core::config_store::{BoundedStoreRead, ConfigValue, finish_bounded_config_read};
    use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
    use edgezero_core::http::HeaderName;
    use edgezero_core::http::{
        HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header, request_builder,
    };
    use edgezero_core::outbound::{
        ContentTypeProvenance, HttpClient, OutboundBatch, OutboundHttpClient, OutboundRequest,
        OutboundResponse,
    };
    use edgezero_core::response_egress_framing::ResponseEgressFramingMode;
    use edgezero_core::store_registry::{ConfigRegistry, ConfigStoreBinding, StoreRegistry};
    use edgezero_core::{
        Deadline, EdgeError, IngressBeginOutcome, IngressDispatchOutcome, IngressFraming,
        IngressHeadAccounting, IngressHeadParts, MonotonicClock, MonotonicInstant,
    };
    use futures::executor::block_on;
    use futures::stream::poll_fn;
    use std::io::Error as IoError;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::task::Poll;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    enum StoreReply {
        Blob(String),
        Expire(Arc<Mutex<MonotonicInstant>>),
        Failure,
        Missing,
        TooLarge,
        Unavailable,
    }

    struct FixtureConfigStore(StoreReply);

    #[async_trait(?Send)]
    impl ConfigStore for FixtureConfigStore {
        async fn get(&self, _key: &str) -> Result<Option<ConfigValue>, ConfigStoreError> {
            match &self.0 {
                StoreReply::Blob(blob) => Ok(Some(blob.clone().into())),
                StoreReply::Expire(_) => Ok(Some(
                    envelope(serde_json::json!({ "bid_cpm": 0.20_f64 })).into(),
                )),
                StoreReply::Failure => Err(ConfigStoreError::internal(IoError::other(
                    "private backend diagnostic",
                ))),
                StoreReply::Missing => Ok(None),
                StoreReply::TooLarge => Err(ConfigStoreError::ValueTooLarge),
                StoreReply::Unavailable => {
                    Err(ConfigStoreError::unavailable("private backend diagnostic"))
                }
            }
        }

        async fn get_bounded(
            &self,
            key: &str,
            clock: &MonotonicClock,
            deadline: Deadline,
            max_backend_bytes: u64,
            max_value_bytes: u64,
        ) -> Result<BoundedStoreRead<ConfigValue>, ConfigStoreError> {
            if deadline.is_expired_at(clock.now()) {
                return Err(ConfigStoreError::DeadlineExceeded);
            }
            if let StoreReply::Expire(now) = &self.0 {
                *now.lock().unwrap() = deadline.instant();
            }
            finish_bounded_config_read(
                self.get(key).await,
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
            assert_eq!(parts.method, Method::GET);
            assert_eq!(
                parts.uri.to_string(),
                "https://signed-router-auction.example/.well-known/trusted-server.json"
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

    fn envelope(data: serde_json::Value) -> String {
        serde_json::to_string(&BlobEnvelope::new(data, "2026-01-01T00:00:00Z".to_owned())).unwrap()
    }

    fn fixture_registry(reply: StoreReply) -> ConfigRegistry {
        StoreRegistry::single_id(
            "mocktioneer_config".to_owned(),
            ConfigStoreBinding {
                handle: ConfigStoreHandle::new(Arc::new(FixtureConfigStore(reply))),
                default_key: "mocktioneer_config".to_owned(),
            },
        )
    }

    fn assert_error(
        response: Response,
        status: StatusCode,
        kind: &str,
        message: &str,
        retry_after: Option<&str>,
    ) {
        assert_eq!(response.status(), status);
        assert_cors(&response);
        assert_eq!(
            response
                .headers()
                .get("retry-after")
                .map(|value| value.to_str().unwrap()),
            retry_after
        );
        let error: serde_json::Value = serde_json::from_slice(&body_bytes(response)).unwrap();
        assert_eq!(error["error"]["kind"], kind);
        assert_eq!(error["error"]["message"], message);
    }

    fn assert_cors(response: &Response) {
        for (name, value) in [
            ("access-control-allow-origin", "*"),
            ("access-control-allow-methods", "GET, POST, OPTIONS"),
            ("access-control-allow-headers", "*, content-type"),
        ] {
            let values: Vec<_> = response.headers().get_all(name).iter().collect();
            assert_eq!(values.len(), 1, "{name}");
            assert_eq!(values[0], value, "{name}");
        }
    }

    #[test]
    fn matched_bid_parse_validation_and_size_errors_have_cors() {
        for (path, cap, invalid) in [
            ("/openrtb2/auction", 1024 * 1024, r#"{"id":"r","imp":[]}"#),
            ("/e/dtb/bid", 1024 * 1024, r#"{"pubId":"p","slots":[]}"#),
            (
                "/adserver/mediate",
                8 * 1024 * 1024,
                r#"{"id":"m","imp":[],"ext":{"bidder_responses":[{"bidder":"a","bids":[]}]}}"#,
            ),
        ] {
            for (payload, status) in [
                (b"{".to_vec(), StatusCode::BAD_REQUEST),
                (
                    invalid.as_bytes().to_vec(),
                    StatusCode::UNPROCESSABLE_ENTITY,
                ),
                (vec![b' '; cap + 1], StatusCode::PAYLOAD_TOO_LARGE),
            ] {
                let mut request = make_request(Method::POST, path, Body::from_bytes(payload));
                request.extensions_mut().insert(config_registry(0.20_f64));
                let response = dispatch(&app(), request);
                assert_eq!(response.status(), status, "{path}");
                assert_cors(&response);
                if status == StatusCode::PAYLOAD_TOO_LARGE {
                    assert!(!response.headers().contains_key(header::RETRY_AFTER));
                    let error: serde_json::Value =
                        serde_json::from_slice(&body_bytes(response)).unwrap();
                    assert_eq!(error["error"]["kind"], "payload_too_large");
                    assert_eq!(error["error"]["message"], "request body too large");
                }
            }
        }
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "keep the complete config wire-contract table together for both bid routes"
    )]
    fn app_config_errors_keep_exact_status_kind_message_and_retry_policy() {
        for path in ["/openrtb2/auction", "/e/dtb/bid"] {
            let good_data = serde_json::json!({ "bid_cpm": 0.20_f64 });
            let mut bad_hash = BlobEnvelope::new(good_data.clone(), "fixed".to_owned());
            bad_hash.sha256 = "private-hash".to_owned();
            let mut future = BlobEnvelope::new(good_data, "fixed".to_owned());
            future.version = 2;
            let deep = format!(
                "{{\"data\":{}0{},\"generated_at\":\"fixed\",\"sha256\":\"unused\",\"version\":1}}",
                "[".repeat(65),
                "]".repeat(65)
            );
            let many_nodes =
                envelope(serde_json::json!({ "bid_cpm": 0.20_f64, "nodes": vec![0_u8; 8193] }));
            for (reply, status, kind, message, retry) in [
                (
                    None,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "no default config store registered; check [stores.config] in edgezero.toml",
                    None,
                ),
                (
                    Some(StoreReply::Missing),
                    StatusCode::SERVICE_UNAVAILABLE,
                    "config_out_of_date",
                    "missing typed app-config blob at key `mocktioneer_config`; run `<app-cli> config push` for this deploy",
                    Some("60"),
                ),
                (
                    Some(StoreReply::Blob(envelope(
                        serde_json::json!({ "bid_cpm": "invalid" }),
                    ))),
                    StatusCode::SERVICE_UNAVAILABLE,
                    "config_out_of_date",
                    "typed app-config is out of date (wrong type or invalid value; value redacted)",
                    Some("60"),
                ),
                (
                    Some(StoreReply::Blob(envelope(
                        serde_json::json!({ "bid_cpm": 0.0_f64 }),
                    ))),
                    StatusCode::SERVICE_UNAVAILABLE,
                    "config_out_of_date",
                    "app config failed validation for field `bid_cpm`",
                    Some("60"),
                ),
                (
                    Some(StoreReply::Blob("{".to_owned())),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config blob is not a valid envelope (details redacted)",
                    None,
                ),
                (
                    Some(StoreReply::Blob(serde_json::to_string(&bad_hash).unwrap())),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config blob failed its integrity check (details redacted)",
                    None,
                ),
                (
                    Some(StoreReply::Blob(serde_json::to_string(&future).unwrap())),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config blob uses envelope version 2, which this build does not understand; redeploy this service with an updated build",
                    None,
                ),
                (
                    Some(StoreReply::Unavailable),
                    StatusCode::SERVICE_UNAVAILABLE,
                    "service_unavailable",
                    "typed app-config store is unavailable",
                    None,
                ),
                (
                    Some(StoreReply::Failure),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config store read failed (details redacted)",
                    None,
                ),
                (
                    Some(StoreReply::TooLarge),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config store value exceeded its read limit",
                    None,
                ),
                (
                    Some(StoreReply::Blob(deep)),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config blob exceeded its JSON structure limit",
                    None,
                ),
                (
                    Some(StoreReply::Blob(many_nodes)),
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "typed app-config blob exceeded its JSON structure limit",
                    None,
                ),
            ] {
                let mut request = make_request(
                    Method::POST,
                    path,
                    Body::from("invalid body is deliberately secondary"),
                );
                if let Some(value) = reply {
                    request.extensions_mut().insert(fixture_registry(value));
                }
                assert_error(dispatch(&app(), request), status, kind, message, retry);
            }
        }
    }

    #[test]
    fn admitted_clock_distinguishes_config_expiry_from_body_expiry() {
        for path in ["/openrtb2/auction", "/e/dtb/bid"] {
            let mut application = app();
            let start = application.monotonic_clock().now();
            let now = Arc::new(Mutex::new(start));
            let clock_source = Arc::clone(&now);
            application
                .set_monotonic_clock(MonotonicClock::new(move || *clock_source.lock().unwrap()));
            let mut config_request = make_request(Method::POST, path, Body::from("{}"));
            config_request
                .extensions_mut()
                .insert(fixture_registry(StoreReply::Expire(Arc::clone(&now))));
            assert_error(
                dispatch(&application, config_request),
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "typed app-config store read deadline exceeded",
                None,
            );
            assert_eq!(
                now.lock().unwrap().checked_duration_since(start),
                Some(Duration::from_secs(2))
            );
        }

        for path in ["/openrtb2/auction", "/e/dtb/bid", "/adserver/mediate"] {
            let mut application = app();
            let start = application.monotonic_clock().now();
            let now = Arc::new(Mutex::new(start));
            let clock_source = Arc::clone(&now);
            application
                .set_monotonic_clock(MonotonicClock::new(move || *clock_source.lock().unwrap()));
            let source = Arc::clone(&now);
            let body = Body::from_stream(poll_fn(move |_| {
                *source.lock().unwrap() = start.checked_add(Duration::from_secs(10)).unwrap();
                Poll::Ready(Some(Ok(b"{}".to_vec().into())))
            }));
            let mut request = make_request(Method::POST, path, body);
            request.extensions_mut().insert(config_registry(0.20_f64));
            assert_error(
                dispatch(&application, request),
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "inbound body read deadline exceeded",
                None,
            );
        }
    }

    #[test]
    fn normalized_head_validation_precedes_routes_and_cors() {
        let application = app();
        let long = format!("/{}", "x".repeat(8193));
        let long_request = make_request(Method::GET, &long, Body::empty());
        let long_response = dispatch(&application, long_request);
        assert_eq!(long_response.status(), StatusCode::URI_TOO_LONG);
        assert!(
            !long_response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let mut header_request = make_request(Method::GET, "/", Body::empty());
        for index in 0_u32..101 {
            header_request.headers_mut().append(
                HeaderName::from_bytes(format!("x-{index}").as_bytes()).unwrap(),
                HeaderValue::from_static("v"),
            );
        }
        let header_response = dispatch(&application, header_request);
        assert_eq!(
            header_response.status(),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
        );
        assert!(
            !header_response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let mut byte_request = make_request(Method::GET, "/", Body::empty());
        byte_request.headers_mut().insert(
            "x-large",
            HeaderValue::from_bytes(&vec![b'a'; 64 * 1024]).unwrap(),
        );
        let byte_response = dispatch(&application, byte_request);
        assert_eq!(
            byte_response.status(),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
        );
        assert!(
            !byte_response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let mut framing_request = make_request(Method::GET, "/", Body::empty());
        framing_request
            .headers_mut()
            .insert("content-length", HeaderValue::from_static("0"));
        framing_request
            .headers_mut()
            .insert("transfer-encoding", HeaderValue::from_static("chunked"));
        let framing_response = dispatch(&application, framing_request);
        assert_eq!(framing_response.status(), StatusCode::BAD_REQUEST);
        assert!(
            !framing_response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }

    #[test]
    fn fallback_errors_are_immediate_and_do_not_poll_the_body() {
        for (method, path, status) in [
            (Method::POST, "/unknown", StatusCode::NOT_FOUND),
            (Method::PUT, "/", StatusCode::METHOD_NOT_ALLOWED),
        ] {
            let polled = Arc::new(AtomicBool::new(false));
            let watch = Arc::clone(&polled);
            let body = Body::from_stream(poll_fn(move |_| {
                watch.store(true, Ordering::SeqCst);
                Poll::Ready(Some(Err(edgezero_core::EdgeError::bad_request(
                    "body must not be read",
                ))))
            }));
            let response = dispatch(&app(), make_request(method, path, body));
            assert_eq!(response.status(), status);
            assert!(!polled.load(Ordering::SeqCst));
            assert!(
                !response
                    .headers()
                    .contains_key("access-control-allow-origin")
            );
            if status == StatusCode::METHOD_NOT_ALLOWED {
                assert_eq!(response.headers().get("allow").unwrap(), "GET, OPTIONS");
            }
        }
    }

    fn app() -> App {
        App::build::<mocktioneer_core::MocktioneerApp>(edgezero_core::PlatformMetadata::default())
            .expect("build app")
    }

    /// Build a `ConfigRegistry` whose default `mocktioneer_config` store holds a
    /// blob envelope for `{ "bid_cpm": <cpm> }` — what `config push` writes.
    /// The auction/APS handlers use the fail-loud `AppConfig` extractor, so the
    /// router needs this bound to serve those routes.
    fn config_registry(bid_cpm: f64) -> ConfigRegistry {
        fixture_registry(StoreReply::Blob(envelope(
            serde_json::json!({ "bid_cpm": bid_cpm }),
        )))
    }

    fn make_request(method: Method, uri: &str, body: Body) -> Request {
        request_builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "mocktioneer.edgecompute.app")
            .body(body)
            .expect("request")
    }

    fn dispatch(app: &App, request: Request) -> Response {
        let start = app.monotonic_clock().now();
        let method = request.method().clone();
        let head = IngressHeadParts::from_request(
            &request,
            IngressHeadAccounting::HostManaged,
            IngressFraming::HostManaged,
        );
        let envelope = if let Err(error) = head.validate_normalized(app.ingress_head_limits()) {
            let IngressDispatchOutcome::Response(envelope) =
                app.detached_ingress_error_egress(error, method, start)
            else {
                panic!("expected detached error response");
            };
            *envelope
        } else {
            let IngressBeginOutcome::Admitted(prepared) =
                app.begin_ingress(head, start).expect("begin ingress")
            else {
                panic!("expected admitted request");
            };
            block_on(app.dispatch_admitted(prepared, request))
        };
        let Ok((prepared, _, mut attempt, clock)) =
            envelope.begin(ResponseEgressFramingMode::ExplicitBufferedLength)
        else {
            panic!("prepare test response");
        };
        assert!(attempt.begin_writing());
        assert!(attempt.complete(clock.now()));
        prepared.into_response()
    }

    fn body_bytes(response: Response) -> Vec<u8> {
        response
            .into_body()
            .into_bytes()
            .expect("buffered body")
            .to_vec()
    }

    #[test]
    fn root_returns_html_with_cors() {
        let app = app();
        let response = dispatch(&app, make_request(Method::GET, "/", Body::empty()));
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.starts_with("text/html"));
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .unwrap()
                .to_str()
                .unwrap(),
            "*"
        );
    }

    #[test]
    fn pixel_sets_cookie_and_is_gif() {
        let app = app();

        let first_response = dispatch(
            &app,
            make_request(Method::GET, "/pixel?pid=first", Body::empty()),
        );
        assert_eq!(first_response.status(), StatusCode::OK);
        let content_type = first_response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_type, "image/gif");
        let cookies: Vec<_> = first_response
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|value| value.to_str().unwrap().to_owned())
            .collect();
        assert!(cookies.iter().any(|cookie| cookie.starts_with("mtkid=")));
        assert!(cookies.iter().any(|cookie| cookie.contains("SameSite=None")
            && cookie.contains("Secure")
            && cookie.contains("HttpOnly")));

        let mut second = make_request(Method::GET, "/pixel?pid=second", Body::empty());
        second
            .headers_mut()
            .insert(header::COOKIE, HeaderValue::from_static("mtkid=abc"));
        let second_response = dispatch(&app, second);
        assert_eq!(second_response.status(), StatusCode::OK);
        assert!(second_response.headers().get("set-cookie").is_none());
    }

    #[test]
    fn openrtb_auction_returns_json() {
        let app = app();
        let body = Body::json(&serde_json::json!({
            "id": "r1",
            "imp": [{"id":"1","banner":{"w":300_i32,"h":250_i32}}]
        }))
        .unwrap();
        let mut request = make_request(Method::POST, "/openrtb2/auction", body);
        request
            .headers_mut()
            .insert(header::HOST, HeaderValue::from_static("test.local"));
        request.extensions_mut().insert(config_registry(0.20_f64));
        let response = dispatch(&app, request);
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_type, "application/json");
        let payload: serde_json::Value = serde_json::from_slice(&body_bytes(response)).unwrap();
        assert_eq!(payload["id"], "r1");
        assert!(payload["seatbid"].is_array());
    }

    fn signed_auction_request(domain: &str) -> (Request, Vec<u8>) {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use ed25519_dalek::{Signer as _, SigningKey};

        let request_id = "signed-router-request";
        let key_id = "signed-router-key";
        let timestamp = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(&[73_u8; 32]);
        // Trusted Server v1.1 signs this exact field order.
        let payload = format!(
            r#"{{"version":"1.1","kid":"{key_id}","host":"{domain}","scheme":"https","id":"{request_id}","ts":{timestamp}}}"#
        );
        let body = Body::json(&serde_json::json!({
            "id": request_id,
            "site": { "domain": domain },
            "imp": [{ "id": "1", "banner": { "w": 300_i32, "h": 250_i32 } }],
            "ext": { "trusted_server": {
                "version": "1.1", "kid": key_id, "request_host": domain,
                "request_scheme": "https", "ts": timestamp,
                "signature": URL_SAFE_NO_PAD.encode(signing_key.sign(payload.as_bytes()).to_bytes())
            } }
        }))
        .unwrap();
        let reply = serde_json::to_vec(&serde_json::json!({ "jwks": { "keys": [{
            "kid": key_id, "x": URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes())
        }] } }))
        .unwrap();
        let mut request = make_request(Method::POST, "/openrtb2/auction", body);
        request.extensions_mut().insert(config_registry(0.35_f64));
        (request, reply)
    }

    fn auction_signature_metadata(response: Response) -> serde_json::Value {
        assert_eq!(response.status(), StatusCode::OK);
        assert_cors(&response);
        let payload: serde_json::Value = serde_json::from_slice(&body_bytes(response)).unwrap();
        let bid = &payload["seatbid"][0]["bid"][0];
        assert_eq!(bid["price"], 0.35_f64);
        let metadata_json = bid["adm"]
            .as_str()
            .unwrap()
            .split_once("<!-- MOCKTIONEER_METADATA\n")
            .unwrap()
            .1
            .split_once("\n-->")
            .unwrap()
            .0;
        let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap();
        metadata["signature"].clone()
    }

    #[test]
    fn signed_auction_verifies_through_router_with_request_http_client() {
        let application = app();
        let calls = Arc::new(AtomicUsize::new(0));
        let (mut request, reply) = signed_auction_request("signed-router-auction.example");
        request
            .extensions_mut()
            .insert(HttpClient::with_client(JwksClient {
                calls: Arc::clone(&calls),
                reply,
            }));
        let signature = auction_signature_metadata(dispatch(&application, request));
        assert_eq!(signature["status"], "Verified");
        assert_eq!(signature["details"]["kid"], "signed-router-key");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // A distinct uncached host prevents the global JWKS cache masking a missing client.
        let (request_without_client, _) = signed_auction_request("signed-router-no-client.example");
        let failure = auction_signature_metadata(dispatch(&application, request_without_client));
        assert_eq!(failure["status"], "Failed");
        assert_eq!(
            failure["details"]["reason"],
            "HTTP error: Proxy not available"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn static_img_svg_and_nonstandard_404() {
        let app = app();
        let svg_response = dispatch(
            &app,
            make_request(
                Method::GET,
                "/static/img/300x250.svg?bid=2.5",
                Body::empty(),
            ),
        );
        assert_eq!(svg_response.status(), StatusCode::OK);
        let content_type = svg_response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_type, "image/svg+xml");
        let svg_body = String::from_utf8(body_bytes(svg_response)).unwrap();
        assert!(svg_body.contains("<svg"));

        let nonstandard_response = dispatch(
            &app,
            make_request(Method::GET, "/static/img/333x222.svg", Body::empty()),
        );
        assert_eq!(
            nonstandard_response.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn static_creatives_html_ok() {
        let app = app();
        let default_response = dispatch(
            &app,
            make_request(Method::GET, "/static/creatives/300x250.html", Body::empty()),
        );
        assert_eq!(default_response.status(), StatusCode::OK);
        let content_type = default_response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.starts_with("text/html"));
        let default_body = String::from_utf8(body_bytes(default_response)).unwrap();
        assert!(default_body.contains("//mocktioneer.edgecompute.app/pixel?pid="));
        assert!(default_body.contains("data-static-pid=\""));
        assert!(!default_body.contains("var jsPid = \""));

        let no_pixel_response = dispatch(
            &app,
            make_request(
                Method::GET,
                "/static/creatives/300x250.html?pixel_html=false",
                Body::empty(),
            ),
        );
        assert_eq!(no_pixel_response.status(), StatusCode::OK);
        let no_pixel_body = String::from_utf8(body_bytes(no_pixel_response)).unwrap();
        assert!(!no_pixel_body.contains("/pixel"));
        assert!(!no_pixel_body.contains("var jsPid = \""));

        let js_pixel_response = dispatch(
            &app,
            make_request(
                Method::GET,
                "/static/creatives/300x250.html?pixel_js=true",
                Body::empty(),
            ),
        );
        assert_eq!(js_pixel_response.status(), StatusCode::OK);
        let js_pixel_body = String::from_utf8(body_bytes(js_pixel_response)).unwrap();
        assert!(js_pixel_body.contains("//mocktioneer.edgecompute.app/pixel?pid="));
        let static_pid = js_pixel_body
            .split("data-static-pid=\"")
            .nth(1)
            .and_then(|segment| segment.split('\"').next())
            .expect("static pid");
        let js_pid = js_pixel_body
            .split("var jsPid = \"")
            .nth(1)
            .and_then(|segment| segment.split('\"').next())
            .expect("js pid");
        assert_ne!(static_pid, js_pid);
    }

    #[test]
    fn click_echoes_params() {
        let app = app();
        let simple_response = dispatch(
            &app,
            make_request(Method::GET, "/click?crid=abc&w=300&h=250", Body::empty()),
        );
        assert_eq!(simple_response.status(), StatusCode::OK);
        let simple_body = String::from_utf8(body_bytes(simple_response)).unwrap();
        assert!(simple_body.contains("abc"));
        assert!(!simple_body.contains("Additional Parameters"));

        let extra_params_response = dispatch(
            &app,
            make_request(
                Method::GET,
                "/click?crid=abc&foo=bar&baz=qux",
                Body::empty(),
            ),
        );
        assert_eq!(extra_params_response.status(), StatusCode::OK);
        let extra_params_body = String::from_utf8(body_bytes(extra_params_response)).unwrap();
        assert!(extra_params_body.contains("Additional Parameters"));
        assert!(extra_params_body.contains("foo"));
        assert!(extra_params_body.contains("bar"));
        assert!(extra_params_body.contains("baz"));
        assert!(extra_params_body.contains("qux"));
    }

    #[test]
    fn ec_sync_endpoints_are_routed() {
        let app = app();

        let sync_start = dispatch(
            &app,
            make_request(
                Method::GET,
                "/sync/start?ts_domain=evil.com%2Fpath",
                Body::empty(),
            ),
        );
        assert_ne!(sync_start.status(), StatusCode::NOT_FOUND);
        assert_eq!(sync_start.status(), StatusCode::BAD_REQUEST);

        let sync_done = dispatch(
            &app,
            make_request(Method::GET, "/sync/done?ts_synced=1", Body::empty()),
        );
        assert_eq!(sync_done.status(), StatusCode::OK);
        assert_eq!(
            sync_done
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "image/gif"
        );

        let resolve = dispatch(
            &app,
            make_request(
                Method::GET,
                "/resolve?ec_id=tooshort&ip=1.2.3.4",
                Body::empty(),
            ),
        );
        assert_ne!(resolve.status(), StatusCode::NOT_FOUND);
        assert!(matches!(
            resolve.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        ));
    }

    #[test]
    fn options_includes_allow_and_cors_headers() {
        let app = app();
        for path in [
            "/openrtb2/auction",
            "/sync/start",
            "/sync/done",
            "/resolve",
            // Introspection routes carry CORS preflight like every other route
            // (regression: these OPTIONS triggers returned 405 before they were
            // added alongside the GET introspection routes).
            "/_mocktioneer/manifest",
            "/_mocktioneer/config",
            "/_mocktioneer/routes",
        ] {
            let response = dispatch(&app, make_request(Method::OPTIONS, path, Body::empty()));
            assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
            let allow = response
                .headers()
                .get(header::ALLOW)
                .unwrap()
                .to_str()
                .unwrap();
            assert!(allow.contains("OPTIONS"), "{path}");
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-methods")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                "GET, POST, OPTIONS",
                "{path}"
            );
        }
    }

    #[test]
    fn introspection_routes_serve_json() {
        let app = app();

        // `routes` — the live route table as [{ "method", "path" }]. Needs no
        // config store.
        let routes = dispatch(
            &app,
            make_request(Method::GET, "/_mocktioneer/routes", Body::empty()),
        );
        assert_eq!(routes.status(), StatusCode::OK);
        let table: serde_json::Value = serde_json::from_slice(&body_bytes(routes)).unwrap();
        let entries = table.as_array().expect("route table is a JSON array");
        assert!(
            entries
                .iter()
                .any(|entry| { entry["path"] == "/openrtb2/auction" && entry["method"] == "POST" })
        );

        // `manifest` — the compiled manifest as JSON. Needs no config store.
        let manifest = dispatch(
            &app,
            make_request(Method::GET, "/_mocktioneer/manifest", Body::empty()),
        );
        assert_eq!(manifest.status(), StatusCode::OK);

        // `config` — the effective app config; reads the default config store,
        // so it needs the same seeded registry the auction tests use.
        let mut request = make_request(Method::GET, "/_mocktioneer/config", Body::empty());
        request.extensions_mut().insert(config_registry(0.20_f64));
        let config = dispatch(&app, request);
        assert_eq!(config.status(), StatusCode::OK);
    }
}

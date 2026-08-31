#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use edgezero_core::app::App;
    use edgezero_core::blob_envelope::BlobEnvelope;
    use edgezero_core::body::Body;
    use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
    use edgezero_core::http::{
        HeaderValue, Method, Request, Response, StatusCode, header, request_builder,
    };
    use edgezero_core::store_registry::{ConfigRegistry, ConfigStoreBinding, StoreRegistry};
    use futures::executor::block_on;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// In-memory config store returning a blob envelope by key.
    struct MapConfigStore(HashMap<String, String>);

    #[async_trait(?Send)]
    impl ConfigStore for MapConfigStore {
        async fn get(&self, key: &str) -> Result<Option<String>, ConfigStoreError> {
            Ok(self.0.get(key).cloned())
        }
    }

    fn app() -> App {
        mocktioneer_core::build_app()
    }

    /// Build a `ConfigRegistry` whose default `mocktioneer_config` store holds a
    /// blob envelope for `{ "bid_cpm": <cpm> }` — what `config push` writes.
    /// The auction/APS handlers use the fail-loud `AppConfig` extractor, so the
    /// router needs this bound to serve those routes.
    fn config_registry(bid_cpm: f64) -> ConfigRegistry {
        let data = serde_json::json!({ "bid_cpm": bid_cpm });
        let blob =
            serde_json::to_string(&BlobEnvelope::new(data, "2026-01-01T00:00:00Z".to_owned()))
                .expect("serialize envelope");
        let store = MapConfigStore(
            [("mocktioneer_config".to_owned(), blob)]
                .into_iter()
                .collect(),
        );
        StoreRegistry::single_id(
            "mocktioneer_config".to_owned(),
            ConfigStoreBinding {
                handle: ConfigStoreHandle::new(Arc::new(store)),
                default_key: "mocktioneer_config".to_owned(),
            },
        )
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
        block_on(app.router().oneshot(request)).expect("router result")
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

use edgezero_core::body::Body;
use edgezero_core::http::{header, request_builder, HeaderValue, Method, StatusCode};
use futures::executor::block_on;

fn app() -> edgezero_core::app::App {
    mocktioneer_core::build_app()
}

fn make_request(method: Method, uri: &str, body: Body) -> edgezero_core::http::Request {
    request_builder()
        .method(method)
        .uri(uri)
        .body(body)
        .expect("request")
}

#[test]
fn root_returns_html_with_cors() {
    let app = app();
    let response = block_on(
        app.router()
            .oneshot(make_request(Method::GET, "/", Body::empty())),
    );
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/html"));
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

    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/pixel?pid=first",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "image/gif");
    let cookies: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert!(cookies.iter().any(|c| c.starts_with("mtkid=")));
    assert!(cookies
        .iter()
        .any(|c| c.contains("SameSite=None") && c.contains("Secure") && c.contains("HttpOnly")));

    let mut second = make_request(Method::GET, "/pixel?pid=second", Body::empty());
    second
        .headers_mut()
        .insert(header::COOKIE, HeaderValue::from_static("mtkid=abc"));
    let response = block_on(app.router().oneshot(second));
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
}

#[test]
fn openrtb_auction_returns_json() {
    let app = app();
    let body = Body::json(&serde_json::json!({
        "id": "r1",
        "imp": [{"id":"1","banner":{"w":300,"h":250}}]
    }))
    .unwrap();
    let mut request = make_request(Method::POST, "/openrtb2/auction", body);
    request
        .headers_mut()
        .insert(header::HOST, HeaderValue::from_static("test.local"));
    let response = block_on(app.router().oneshot(request));
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/json");
    let payload: serde_json::Value = serde_json::from_slice(response.body().as_bytes()).unwrap();
    assert_eq!(payload["id"], "r1");
    assert!(payload["seatbid"].is_array());
}

#[test]
fn static_img_svg_and_nonstandard_404() {
    let app = app();
    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/static/img/300x250.svg?bid=2.5",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "image/svg+xml");
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(body.contains("<svg"));

    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/static/img/333x222.svg",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn static_creatives_html_ok() {
    let app = app();
    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/static/creatives/300x250.html",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/html"));
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(body.contains("//mocktioneer.edgecompute.app/pixel?pid="));
    assert!(body.contains("data-static-pid=\""));
    assert!(!body.contains("var jsPid = \""));

    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/static/creatives/300x250.html?pixel_html=false",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(!body.contains("/pixel"));
    assert!(!body.contains("var jsPid = \""));

    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/static/creatives/300x250.html?pixel_js=true",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(body.contains("//mocktioneer.edgecompute.app/pixel?pid="));
    let static_pid = body
        .split("data-static-pid=\"")
        .nth(1)
        .and_then(|s| s.split('\"').next())
        .expect("static pid");
    let js_pid = body
        .split("var jsPid = \"")
        .nth(1)
        .and_then(|s| s.split('\"').next())
        .expect("js pid");
    assert_ne!(static_pid, js_pid);
}

#[test]
fn click_echoes_params() {
    let app = app();
    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/click?crid=abc&w=300&h=250",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(body.contains("abc"));
    assert!(!body.contains("Additional Parameters"));

    let response = block_on(app.router().oneshot(make_request(
        Method::GET,
        "/click?crid=abc&foo=bar&baz=qux",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
    assert!(body.contains("Additional Parameters"));
    assert!(body.contains("foo"));
    assert!(body.contains("bar"));
    assert!(body.contains("baz"));
    assert!(body.contains("qux"));
}

#[test]
fn options_includes_allow_and_cors_headers() {
    let app = app();
    let response = block_on(app.router().oneshot(make_request(
        Method::OPTIONS,
        "/openrtb2/auction",
        Body::empty(),
    )));
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let allow = response
        .headers()
        .get(header::ALLOW)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allow.contains("POST"));
    assert!(allow.contains("OPTIONS"));
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-methods")
            .unwrap()
            .to_str()
            .unwrap(),
        "GET, POST, OPTIONS"
    );
}

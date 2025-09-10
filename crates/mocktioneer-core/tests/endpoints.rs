use anyedge_core::{header, Method, Request};

// Build the application from the crate under test
fn app() -> anyedge_core::App {
    mocktioneer_core::build_app()
}

#[test]
fn root_returns_html_with_cors() {
    let app = app();
    let res = app.handle(Request::new(Method::GET, "/"));
    assert_eq!(res.status.as_u16(), 200);
    let ct = res
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/html"));
    // CORS headers from mocktioneer middleware
    assert_eq!(
        res.headers
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

    // First request without cookie should set it
    let res = app.handle(Request::new(Method::GET, "/pixel"));
    assert_eq!(res.status.as_u16(), 200);
    let ct = res
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "image/gif");
    let set_cookie = res.headers.get_all("set-cookie");
    let cookies: Vec<_> = set_cookie
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert!(cookies.iter().any(|c| c.starts_with("mtkid=")));
    assert!(cookies
        .iter()
        .any(|c| c.contains("SameSite=None") && c.contains("Secure") && c.contains("HttpOnly")));

    // Second request with cookie should not set again
    let mut req2 = Request::new(Method::GET, "/pixel");
    req2.set_header("Cookie", "mtkid=abc");
    let res2 = app.handle(req2);
    assert_eq!(res2.status.as_u16(), 200);
    assert!(res2.headers.get("set-cookie").is_none());
}

#[test]
fn openrtb_auction_returns_json() {
    let app = app();
    let body = serde_json::json!({
        "id": "r1",
        "imp": [{"id":"1","banner":{"w":300,"h":250}}]
    })
    .to_string();
    let mut req = Request::new(Method::POST, "/openrtb2/auction").with_body(body);
    // Optional: Host header for building creatives (not required for this test)
    req.set_header("Host", "test.local");
    let res = app.handle(req);
    assert_eq!(res.status.as_u16(), 200);
    let ct = res
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/json");
    let v: serde_json::Value = serde_json::from_slice(&res.body).unwrap();
    assert_eq!(v["id"], "r1");
    assert!(v["seatbid"].is_array());
}

#[test]
fn static_img_svg_and_nonstandard_404() {
    let app = app();

    let mut req = Request::new(Method::GET, "/static/img/300x250.svg");
    req.query_params.insert("bid".into(), "2.5".into());
    let res = app.handle(req);
    assert_eq!(res.status.as_u16(), 200);
    let ct = res
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "image/svg+xml");
    let body = String::from_utf8(res.body).unwrap();
    assert!(body.contains("<svg"));

    let res_404 = app.handle(Request::new(Method::GET, "/static/img/333x222.svg"));
    assert_eq!(res_404.status.as_u16(), 404);
}

#[test]
fn static_creatives_html_ok() {
    let app = app();
    let res = app.handle(Request::new(Method::GET, "/static/creatives/300x250.html"));
    assert_eq!(res.status.as_u16(), 200);
    let ct = res
        .headers
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/html"));
    let body = String::from_utf8(res.body).unwrap();
    assert!(body.contains("/pixel"), "default should include pixel");

    let mut req2 = Request::new(Method::GET, "/static/creatives/300x250.html");
    req2.query_params.insert("pixel".into(), "false".into());
    let res2 = app.handle(req2);
    assert_eq!(res2.status.as_u16(), 200);
    let body2 = String::from_utf8(res2.body).unwrap();
    assert!(
        !body2.contains("/pixel"),
        "pixel=false should disable pixel"
    );
}

#[test]
fn click_echoes_params() {
    let app = app();
    let mut req = Request::new(Method::GET, "/click");
    req.query_params.insert("crid".into(), "abc".into());
    req.query_params.insert("w".into(), "300".into());
    req.query_params.insert("h".into(), "250".into());
    let res = app.handle(req);
    assert_eq!(res.status.as_u16(), 200);
    let body = String::from_utf8(res.body).unwrap();
    assert!(body.contains("abc"));
}

#[test]
fn options_includes_allow_and_cors_headers() {
    let app = app();
    let res = app.handle(Request::new(Method::OPTIONS, "/openrtb2/auction"));
    assert_eq!(res.status.as_u16(), 204);
    // Router-provided Allow header should list OPTIONS and the registered method (POST)
    let allow = res
        .headers
        .get(header::ALLOW)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(allow.contains("POST"));
    assert!(allow.contains("OPTIONS"));
    // CORS headers from middleware
    assert_eq!(
        res.headers
            .get("access-control-allow-methods")
            .unwrap()
            .to_str()
            .unwrap(),
        "GET, POST, OPTIONS"
    );
}

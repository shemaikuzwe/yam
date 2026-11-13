use std::path::PathBuf;

use futures::TryStreamExt;
use yam_router::{Router, RouterConfig};
use yam_server::{Body, Handler, Request, Response, StatusCode};

fn request(path: &str, headers: &[(&str, &str)]) -> Request {
    let headers: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();
    let mut request = Request::new();
    request
        .parse(format!("GET {path} HTTP/1.1\r\n{headers}\r\n").as_bytes())
        .expect("request should be valid");
    request
}

fn public_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(dir.join("css")).unwrap();
    std::fs::write(dir.join("css/app.css"), "body{}").unwrap();
    dir
}

async fn get(router: &Router, path: &str, headers: &[(&str, &str)]) -> Response {
    router
        .call(request(path, headers))
        .await
        .expect("handler should succeed")
}

#[tokio::test]
async fn should_serve_file_with_cache_headers() {
    let mut router = Router::new(RouterConfig::default());
    router.static_files("/assets", public_dir("serve"));

    let response = get(&router, "/assets/css/app.css", &[]).await;

    assert_eq!(response.status, StatusCode::Ok);
    assert_eq!(
        response.headers.get("content-type"),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(response.headers.get("content-length"), Some("6"));
    assert!(response.headers.get("etag").is_some());
    assert!(response.headers.get("last-modified").is_some());
    let Body::Stream(stream) = response.body else {
        panic!("file should be streamed");
    };
    let chunks: Vec<Vec<u8>> = stream.try_collect().await.unwrap();
    assert_eq!(chunks.concat(), b"body{}");
}

#[tokio::test]
async fn should_not_apply_route_prefix() {
    let mut router = Router::new(RouterConfig {
        route_prefix: "/api".into(),
        ..Default::default()
    });
    router.static_files("/assets", public_dir("prefix"));

    assert_eq!(
        get(&router, "/assets/css/app.css", &[]).await.status,
        StatusCode::Ok
    );
    assert_eq!(
        get(&router, "/api/assets/css/app.css", &[]).await.status,
        StatusCode::NotFound
    );
}

#[tokio::test]
async fn should_return_not_found_for_missing_file_directory_and_traversal() {
    let mut router = Router::new(RouterConfig::default());
    router.static_files("/assets", public_dir("missing").join("css"));

    for path in [
        "/assets/nope.css",
        "/assets/..%2Fcss/app.css",
        "/assets/../css/app.css",
    ] {
        assert_eq!(
            get(&router, path, &[]).await.status,
            StatusCode::NotFound,
            "{path}"
        );
    }
}

#[tokio::test]
async fn should_answer_not_modified_for_matching_validators() {
    let mut router = Router::new(RouterConfig::default());
    router.static_files("/assets", public_dir("conditional"));
    let first = get(&router, "/assets/css/app.css", &[]).await;
    let etag = first.headers.get("etag").unwrap();
    let last_modified = first.headers.get("last-modified").unwrap();

    let by_etag = get(&router, "/assets/css/app.css", &[("if-none-match", etag)]).await;
    assert_eq!(by_etag.status, StatusCode::NotModified);
    assert_eq!(by_etag.body, b"");

    let by_date = get(
        &router,
        "/assets/css/app.css",
        &[("if-modified-since", last_modified)],
    )
    .await;
    assert_eq!(by_date.status, StatusCode::NotModified);

    // If-None-Match wins: a stale tag means a full response despite a fresh date.
    let stale = get(
        &router,
        "/assets/css/app.css",
        &[
            ("if-none-match", "W/\"stale\""),
            ("if-modified-since", last_modified),
        ],
    )
    .await;
    assert_eq!(stale.status, StatusCode::Ok);
}

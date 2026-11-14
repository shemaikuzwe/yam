use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use yam_client::{Body, HttpClient, HttpClientConfig, RequestOptions};

type Seen = Arc<Mutex<Vec<String>>>;

/// Serves canned responses by request line and records every request it receives.
async fn spawn_server(routes: &'static [(&'static str, &'static str)]) -> (SocketAddr, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let recorded = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(serve_connection(stream, routes, Arc::clone(&recorded)));
        }
    });
    (address, seen)
}

async fn serve_connection(
    mut stream: TcpStream,
    routes: &'static [(&'static str, &'static str)],
    seen: Seen,
) {
    let mut buf = Vec::new();
    loop {
        let head_end = loop {
            if let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
                break end + 4;
            }
            let mut chunk = [0; 1024];
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(read) => buf.extend_from_slice(&chunk[..read]),
            }
        };
        let head = String::from_utf8(buf[..head_end].to_vec()).unwrap();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .map_or(0, |length| length.parse::<usize>().unwrap());
        while buf.len() < head_end + length {
            let mut chunk = [0; 1024];
            let read = stream.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..read]);
        }
        let body = String::from_utf8(buf[head_end..head_end + length].to_vec()).unwrap();
        buf.drain(..head_end + length);
        seen.lock().unwrap().push(format!("{head}{body}"));

        let request_line = head.lines().next().unwrap();
        let response = routes
            .iter()
            .find(|(line, _)| request_line.starts_with(line))
            .map_or(
                "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n",
                |(_, response)| response,
            );
        stream.write_all(response.as_bytes()).await.unwrap();
    }
}

fn client(address: SocketAddr, max_redirects: usize) -> HttpClient {
    HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{address}")),
        timeout: Some(Duration::from_secs(1)),
        headers: vec![("authorization".into(), "Bearer secret".into())],
        max_redirects,
        ..Default::default()
    })
}

const FOUND: &str = "HTTP/1.1 302 Found\r\nlocation: /next\r\ncontent-length: 0\r\n\r\n";
const SEE_OTHER: &str = "HTTP/1.1 303 See Other\r\nlocation: /next\r\ncontent-length: 0\r\n\r\n";
const TEMPORARY: &str =
    "HTTP/1.1 307 Temporary Redirect\r\nlocation: /next\r\ncontent-length: 0\r\n\r\n";
const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok";

#[tokio::test]
async fn should_follow_redirect() {
    static ROUTES: [(&str, &str); 2] = [("GET /start ", FOUND), ("GET /next ", OK)];
    let (address, seen) = spawn_server(&ROUTES).await;

    let response = client(address, 10)
        .get("/start", RequestOptions::default())
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.text().unwrap(), "ok");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[1].contains("authorization: Bearer secret\r\n"));
}

#[tokio::test]
async fn should_switch_post_to_get_on_see_other() {
    static ROUTES: [(&str, &str); 2] = [("POST /start ", SEE_OTHER), ("GET /next ", OK)];
    let (address, seen) = spawn_server(&ROUTES).await;
    let options = RequestOptions {
        body: Some(Body::text("payload")),
        ..Default::default()
    };

    let response = client(address, 10).post("/start", options).await.unwrap();

    assert_eq!(response.status, 200);
    let seen = seen.lock().unwrap();
    assert!(seen[1].starts_with("GET /next "));
    assert!(!seen[1].contains("payload"));
    assert!(!seen[1].contains("content-type"));
}

#[tokio::test]
async fn should_resend_body_on_temporary_redirect() {
    static ROUTES: [(&str, &str); 2] = [("POST /start ", TEMPORARY), ("POST /next ", OK)];
    let (address, seen) = spawn_server(&ROUTES).await;
    let options = RequestOptions {
        body: Some(Body::text("payload")),
        ..Default::default()
    };

    let response = client(address, 10).post("/start", options).await.unwrap();

    assert_eq!(response.status, 200);
    let seen = seen.lock().unwrap();
    assert!(seen[1].starts_with("POST /next "));
    assert!(seen[1].ends_with("payload"));
}

#[tokio::test]
async fn should_return_redirect_when_following_is_disabled() {
    static ROUTES: [(&str, &str); 1] = [("GET /start ", FOUND)];
    let (address, seen) = spawn_server(&ROUTES).await;

    let response = client(address, 0)
        .get("/start", RequestOptions::default())
        .await
        .unwrap();

    assert_eq!(response.status, 302);
    assert_eq!(response.header("location"), Some("/next"));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn should_drop_credentials_on_cross_origin_redirect() {
    static OTHER: [(&str, &str); 1] = [("GET /next ", OK)];
    let (other, other_seen) = spawn_server(&OTHER).await;
    // Leaked so the redirect target can live in a `'static` route table.
    let location: &'static str = Box::leak(
        format!("HTTP/1.1 302 Found\r\nlocation: http://{other}/next\r\ncontent-length: 0\r\n\r\n")
            .into_boxed_str(),
    );
    let routes: &'static [(&str, &str)] = Box::leak(Box::new([("GET /start ", location)]));
    let (address, _) = spawn_server(routes).await;

    let response = client(address, 10)
        .get("/start", RequestOptions::default())
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    let other_seen = other_seen.lock().unwrap();
    assert!(!other_seen[0].contains("authorization"));
}

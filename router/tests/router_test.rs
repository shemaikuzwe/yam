use std::net::SocketAddr;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use yam_router::router::{Router, RouterConfig};

async fn spawn_router(router: Router) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener
        .local_addr()
        .expect("listener should have an address");
    let server = tokio::spawn(async move {
        router.serve(listener).await.expect("server should run");
    });

    (address, server)
}

async fn send_request(address: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(address)
        .await
        .expect("client should connect");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request should be sent");

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("response should be read");
    String::from_utf8(response).expect("response should be valid UTF-8")
}

#[tokio::test]
async fn should_route_request_with_path_parameter() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users/{id}", |request| async move {
        request.param("id").unwrap_or("missing").to_string()
    });
    let (address, server) = spawn_router(router).await;

    let response = send_request(address, "GET /users/42 HTTP/1.1\r\n\r\n").await;
    server.abort();

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.ends_with("\r\n\r\n42"));
}

#[tokio::test]
async fn should_return_not_found_response() {
    let router = Router::new(RouterConfig::default());
    let (address, server) = spawn_router(router).await;

    let response = send_request(address, "GET /missing HTTP/1.1\r\n\r\n").await;
    server.abort();

    assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert!(response.ends_with("\r\n\r\nNot Found"));
}

#[tokio::test]
async fn should_return_method_not_allowed_response() {
    let mut router = Router::new(RouterConfig::default());
    router.get("/users", |_| async { "users" });
    let (address, server) = spawn_router(router).await;

    let response = send_request(address, "POST /users HTTP/1.1\r\n\r\n").await;
    server.abort();

    assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
    assert!(response.ends_with("\r\n\r\nMethod not allowed"));
}

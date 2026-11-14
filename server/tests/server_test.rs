use std::{net::SocketAddr, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};
use yam_server::{Request, Response, Server, ServerConfig};

async fn start(config: ServerConfig) -> (SocketAddr, oneshot::Sender<()>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let handler = |request: Request| async move {
        if request.path() == "/slow" {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let ip = request.remote_addr().map(|addr| addr.ip().to_string());
        Response::new().send(format!("{} {}", ip.unwrap_or_default(), request.body.len()))
    };
    let server = tokio::spawn(async move {
        let config = ServerConfig {
            shutdown: Some(Box::pin(async {
                let _ = stopped.await;
            })),
            ..config
        };
        Server::serve(listener, handler, config)
        .await
        .unwrap();
    });
    (address, stop, server)
}

async fn read_all(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8(response).unwrap()
}

#[tokio::test]
async fn should_finish_in_flight_request_on_shutdown() {
    let (address, stop, server) = start(ServerConfig::default()).await;
    let mut client = TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();

    let response = read_all(&mut client).await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("connection: close\r\n"));
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .expect("server should stop after the request")
        .unwrap();
    assert!(TcpStream::connect(address).await.is_err());
}

#[tokio::test]
async fn should_close_idle_connections_on_shutdown() {
    let (address, stop, server) = start(ServerConfig::default()).await;
    let mut idle = TcpStream::connect(address).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .expect("idle connection should not hold the server open")
        .unwrap();
    assert_eq!(read_all(&mut idle).await, "");
}

#[tokio::test]
async fn should_send_100_continue_before_body() {
    let (address, _stop, _server) = start(ServerConfig::default()).await;
    let mut client = TcpStream::connect(address).await.unwrap();
    client
        .write_all(
            b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\
              Expect: 100-continue\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();

    let mut interim = [0; 25];
    client.read_exact(&mut interim).await.unwrap();
    assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");

    client.write_all(b"hello").await.unwrap();
    let response = read_all(&mut client).await;
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.ends_with("127.0.0.1 5"));
}

#[tokio::test]
async fn should_reject_body_over_limit_without_continue() {
    let (address, _stop, _server) = start(ServerConfig {
        max_body_size: 4,
        ..Default::default()
    })
    .await;
    let mut client = TcpStream::connect(address).await.unwrap();
    client
        .write_all(
            b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\
              Expect: 100-continue\r\n\r\n",
        )
        .await
        .unwrap();

    let response = read_all(&mut client).await;
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
}

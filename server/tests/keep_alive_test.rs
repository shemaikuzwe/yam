use std::sync::Arc;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use yam_server::{Handler, Request, server::handle_request};

#[tokio::test]
async fn serves_two_requests_on_one_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    let client = tokio::spawn(async move {
        tokio::net::TcpStream::connect(address).await.unwrap()
    });
    let (server_stream, _) = listener.accept().await.unwrap();
    let mut client = client.await.unwrap();

    let handler: Arc<dyn Handler> = Arc::new(|request: Request| async move {
        request.path().to_string()
    });
    let server = tokio::spawn(handle_request(server_stream, handler));

    client
        .write_all(
            b"GET /first HTTP/1.1\r\nHost: localhost\r\n\r\n\
              GET /second HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();

    let mut responses = Vec::new();
    client.read_to_end(&mut responses).await.unwrap();
    server.await.unwrap().unwrap();

    let responses = String::from_utf8(responses).unwrap();
    assert_eq!(responses.matches("HTTP/1.1 200 OK").count(), 2);
    assert!(responses.contains("connection: keep-alive\r\n"));
    assert!(responses.contains("connection: close\r\n"));
}

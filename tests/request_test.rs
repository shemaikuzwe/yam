use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use http_server::request::RequestReader;
use tokio::io::{AsyncRead, ReadBuf};

struct ChunkReader {
    data: Vec<u8>,
    position: usize,
    num_bytes_per_read: usize,
}

impl ChunkReader {
    fn new(data: &str, num_bytes_per_read: usize) -> Self {
        Self {
            data: data.as_bytes().to_vec(),
            position: 0,
            num_bytes_per_read,
        }
    }
}

impl AsyncRead for ChunkReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.position >= self.data.len() {
            return Poll::Ready(Ok(())); // EOF
        }

        let remaining = &self.data[self.position..];

        let amount = remaining
            .len()
            .min(self.num_bytes_per_read)
            .min(buffer.remaining());

        buffer.put_slice(&remaining[..amount]);
        self.position += amount;

        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn test_full_request() {
    let data = concat!(
        "POST /submit HTTP/1.1\r\n",
        "Host: localhost:42069\r\n",
        "Content-Length: 13\r\n",
        "\r\n",
        "hello world!\n",
    );
    let reader = ChunkReader::new(data, 3);
    let mut request_reader = RequestReader::new(reader);
    let request = request_reader
        .handle_request()
        .await
        .expect("request should parse successfully");
    let request_line = request.request_line.expect("Should have a request line");
    let host = request
        .headers
        .get("host")
        .expect("Should have host header");
    assert_eq!(request_line.method, "POST");
    assert_eq!(request_line.request_target, "/submit");
    assert_eq!(host, "localhost:42069");
    assert_eq!(request.body, b"hello world!\n");
}

#[tokio::test]
async fn multiple_request_in_stream() {
    let data = concat!(
        "GET / HTTP/1.1\r\n",
        "Host: localhost\r\n",
        "\r\n",
        "GET /about HTTP/1.1\r\n",
        "Host: localhost\r\n",
        "\r\n",
    );
    let reader = ChunkReader::new(data, 3);
    let mut request_reader = RequestReader::new(reader);
    let request1 = request_reader
        .handle_request()
        .await
        .expect("Should parse request 1");
    let request2 = request_reader
        .handle_request()
        .await
        .expect("Should parse request 2");
    let request_line = request1.request_line.expect("Should have a request line");
    assert_eq!(request_line.method, "GET");
    assert_eq!(request_line.request_target, "/");
    let request_line2 = request2.request_line.expect("Should have request line");

    assert_eq!(request_line2.method, "GET");
    assert_eq!(request_line2.request_target, "/about")
}

#[tokio::test]
async fn should_handle_request_with_no_content_length() {
    let data = concat!("POST / HTTP/1.1\r\n", "Host: localhost\r\n", "\r\n",);
    let reader = ChunkReader::new(data, 10);

    let mut request_reader = RequestReader::new(reader);
    let result = request_reader
        .handle_request()
        .await
        .expect("Request to be parsed");
    assert_eq!(result.body.len(), 0);
}
#[tokio::test]

async fn should_handle_request_with_zero_content_length() {
    let data = concat!(
        "GET / HTTP/1.1\r\n",
        "Host: localhost\r\n",
        "Content-Length: 0\r\n",
        "\r\n",
    );
    let reader = ChunkReader::new(data, 10);
    let mut request_reader = RequestReader::new(reader);
    let result = request_reader.handle_request().await;
    assert!(result.is_ok());
}
#[tokio::test]
async fn should_handle_large_content_length() {
    let data = format!(
        "POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
        1000,
        "x".repeat(1000)
    );
    let reader = ChunkReader::new(&data, 50);

    let mut request_reader = RequestReader::new(reader);
    let result = request_reader
        .handle_request()
        .await
        .expect("Request to be parsed");
    assert_eq!(result.body.len(), 1000);
}

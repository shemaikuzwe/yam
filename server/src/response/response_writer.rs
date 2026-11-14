use std::io::{self, Write as _};

use futures::StreamExt;
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::response::{Body, BodyStream, HttpError, Response, StatusCode};

const INLINE_BODY_LIMIT: usize = 16 * 1024;

#[derive(Debug)]
pub struct ResponseWriter<W: AsyncWrite> {
    writer: W,
    head_only: bool,
}

impl<W: AsyncWrite + Unpin> ResponseWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            head_only: false,
        }
    }
    /// Writes the status line and headers but no body, as required for HEAD requests.
    pub fn head_only(mut self, head_only: bool) -> Self {
        self.head_only = head_only;
        self
    }
    pub async fn send_response(mut self, response: Response) -> Result<(), HttpError> {
        let mut buffer = Vec::with_capacity(256);
        write_response(&mut self.writer, &mut buffer, response, self.head_only).await
    }
    pub async fn send(self, body: impl AsRef<[u8]>) -> Result<(), HttpError> {
        self.send_response(Response::new().send(body.as_ref()))
            .await
    }
    pub async fn json(self, body: &impl Serialize) -> Result<(), HttpError> {
        let response = Response::new().json(body);
        self.send_response(response).await
    }
}

fn status_line(status: StatusCode) -> &'static str {
    match status {
        StatusCode::Ok => "HTTP/1.1 200 OK\r\n",
        StatusCode::Created => "HTTP/1.1 201 Created\r\n",
        StatusCode::Accepted => "HTTP/1.1 202 Accepted\r\n",
        StatusCode::NoContent => "HTTP/1.1 204 No Content\r\n",
        StatusCode::MovedPermanently => "HTTP/1.1 301 Moved Permanently\r\n",
        StatusCode::Found => "HTTP/1.1 302 Found\r\n",
        StatusCode::SeeOther => "HTTP/1.1 303 See Other\r\n",
        StatusCode::NotModified => "HTTP/1.1 304 Not Modified\r\n",
        StatusCode::TemporaryRedirect => "HTTP/1.1 307 Temporary Redirect\r\n",
        StatusCode::PermanentRedirect => "HTTP/1.1 308 Permanent Redirect\r\n",
        StatusCode::BadRequest => "HTTP/1.1 400 Bad Request\r\n",
        StatusCode::Unauthorized => "HTTP/1.1 401 Unauthorized\r\n",
        StatusCode::Forbidden => "HTTP/1.1 403 Forbidden\r\n",
        StatusCode::NotFound => "HTTP/1.1 404 Not Found\r\n",
        StatusCode::MethodNotAllowed => "HTTP/1.1 405 Method Not Allowed\r\n",
        StatusCode::NotAcceptable => "HTTP/1.1 406 Not Acceptable\r\n",
        StatusCode::RequestTimeout => "HTTP/1.1 408 Request Timeout\r\n",
        StatusCode::Conflict => "HTTP/1.1 409 Conflict\r\n",
        StatusCode::ContentTooLarge => "HTTP/1.1 413 Content Too Large\r\n",
        StatusCode::UnsupportedMediaType => "HTTP/1.1 415 Unsupported Media Type\r\n",
        StatusCode::UnprocessableContent => "HTTP/1.1 422 Unprocessable Content\r\n",
        StatusCode::TooManyRequests => "HTTP/1.1 429 Too Many Requests\r\n",
        StatusCode::InternalServerError => "HTTP/1.1 500 Internal Server Error\r\n",
        StatusCode::NotImplemented => "HTTP/1.1 501 Not Implemented\r\n",
        StatusCode::BadGateway => "HTTP/1.1 502 Bad Gateway\r\n",
        StatusCode::ServiceUnavailable => "HTTP/1.1 503 Service Unavailable\r\n",
        StatusCode::GatewayTimeout => "HTTP/1.1 504 Gateway Timeout\r\n",
        StatusCode::HttpVersionNotSupported => "HTTP/1.1 505 HTTP Version Not Supported\r\n",
    }
}

/// Serializes `response` to `writer`.
///
pub(crate) async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    buffer: &mut Vec<u8>,
    response: Response,
    head_only: bool,
) -> Result<(), HttpError> {
    let Response {
        status,
        mut headers,
        body,
    } = response;
    let chunked = match &body {
        Body::Bytes(bytes) => {
            headers.set("content-length", bytes.len().to_string());
            false
        }
        // A stream with a known size (e.g. a file) keeps its content-length.
        Body::Stream(_) if headers.get("content-length").is_some() => false,
        Body::Stream(_) => {
            headers.set("transfer-encoding", "chunked".to_string());
            true
        }
    };

    buffer.clear();
    buffer.extend_from_slice(status_line(status).as_bytes());
    for (name, value) in headers.iter() {
        buffer.extend_from_slice(name.as_bytes());
        buffer.extend_from_slice(b": ");
        buffer.extend_from_slice(value.as_bytes());
        buffer.extend_from_slice(b"\r\n");
    }
    buffer.extend_from_slice(b"\r\n");

    match body {
        _ if head_only => writer.write_all(buffer).await?,
        Body::Bytes(bytes) if bytes.len() <= INLINE_BODY_LIMIT => {
            buffer.extend_from_slice(&bytes);
            writer.write_all(buffer).await?;
        }
        Body::Bytes(bytes) => {
            writer.write_all(buffer).await?;
            writer.write_all(&bytes).await?;
        }
        Body::Stream(stream) => write_stream(writer, buffer, stream, chunked).await?,
    }
    writer.flush().await?;
    Ok(())
}

/// Writes a streamed body. `buffer` holds the unsent head, which leaves with the first chunk.
async fn write_stream<W: AsyncWrite + Unpin>(
    writer: &mut W,
    buffer: &mut Vec<u8>,
    mut stream: BodyStream,
    chunked: bool,
) -> io::Result<()> {
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if !chunked {
            buffer.extend_from_slice(&chunk);
        } else if !chunk.is_empty() {
            // <size-in-hex>\r\n<data>\r\n
            write!(buffer, "{:x}\r\n", chunk.len())?;
            buffer.extend_from_slice(&chunk);
            buffer.extend_from_slice(b"\r\n");
        }
        // Push each chunk out as it arrives so streams aren't held in the buffer.
        writer.write_all(buffer).await?;
        writer.flush().await?;
        buffer.clear();
    }
    if chunked {
        buffer.extend_from_slice(b"0\r\n\r\n");
    }
    writer.write_all(buffer).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::Cookie;

    #[tokio::test]
    async fn should_send_complete_response() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(
                Response::new()
                    .status(StatusCode::Ok)
                    .set("content-type", "text/plain")
                    .send("Hello"),
            )
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();

        assert!(output.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(output.contains("content-type: text/plain\r\n"));
        assert!(output.contains("content-length: 5\r\n"));
        assert!(output.ends_with("\r\nHello"));
    }
    #[tokio::test]
    async fn should_set_powered_by_header() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(
                Response::new()
                    .status(StatusCode::Ok)
                    .set("content-type", "text/plain")
                    .send("Hello"),
            )
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("x-powered-by: Yam"));
    }
    #[tokio::test]
    async fn should_send_binary_body() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(Response::new().send(vec![1, 2, 3, 4]))
            .await
            .unwrap();

        assert!(output.ends_with(&[1, 2, 3, 4]));
    }
    #[tokio::test]
    async fn should_send_chunked_stream_response() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(Response::new().stream_response(futures::stream::iter([
                b"hello".to_vec(),
                b" world".to_vec(),
            ])))
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("transfer-encoding: chunked\r\n"));
        assert!(!output.contains("content-length:"));
        assert!(output.ends_with("\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"));
    }
    #[tokio::test]
    async fn should_send_stream_with_content_length_unchunked() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(
                Response::new()
                    .set("content-length", "11")
                    .stream_response(futures::stream::iter(["hello", " world"])),
            )
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();

        assert!(!output.contains("transfer-encoding"));
        assert!(output.contains("content-length: 11\r\n"));
        assert!(output.ends_with("\r\n\r\nhello world"));
    }
    #[tokio::test]
    async fn should_abort_stream_on_error() {
        let mut output = Vec::new();
        let chunks: [io::Result<&str>; 2] = [Ok("hello"), Err(io::Error::other("disk"))];
        let result = ResponseWriter::new(&mut output)
            .send_response(Response::new().stream_response(futures::stream::iter(chunks)))
            .await;

        assert!(result.is_err());
        assert!(!String::from_utf8(output).unwrap().ends_with("0\r\n\r\n"));
    }
    #[tokio::test]
    async fn should_skip_stream_body_for_head() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .head_only(true)
            .send_response(Response::new().stream_response(futures::stream::iter(["hello"])))
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("transfer-encoding: chunked\r\n"));
        assert!(output.ends_with("\r\n\r\n"));
        assert!(!output.contains("hello"));
    }

    #[tokio::test]
    async fn should_send_multiple_cookies_as_separate_headers() {
        let mut output = Vec::new();
        let response = Response::new()
            .cookie(Cookie::new("session", "abc123").http_only(true))
            .cookie(Cookie::new("theme", "dark"));

        ResponseWriter::new(&mut output)
            .send_response(response)
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("set-cookie: session=abc123; SameSite=Lax; HttpOnly\r\n"));
        assert!(output.contains("set-cookie: theme=dark; SameSite=Lax\r\n"));
    }
}

use std::io;

use futures::StreamExt;
use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::response::{Body, BodyStream, HttpError, Response, StatusCode};

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
    async fn write_status_line(&mut self, status: StatusCode) -> io::Result<()> {
        let (code, reason) = match status {
            StatusCode::Ok => (200, "OK"),
            StatusCode::Created => (201, "Created"),
            StatusCode::Accepted => (202, "Accepted"),
            StatusCode::NoContent => (204, "No Content"),
            StatusCode::MovedPermanently => (301, "Moved Permanently"),
            StatusCode::Found => (302, "Found"),
            StatusCode::SeeOther => (303, "See Other"),
            StatusCode::NotModified => (304, "Not Modified"),
            StatusCode::TemporaryRedirect => (307, "Temporary Redirect"),
            StatusCode::PermanentRedirect => (308, "Permanent Redirect"),
            StatusCode::BadRequest => (400, "Bad Request"),
            StatusCode::Unauthorized => (401, "Unauthorized"),
            StatusCode::Forbidden => (403, "Forbidden"),
            StatusCode::NotFound => (404, "Not Found"),
            StatusCode::MethodNotAllowed => (405, "Method Not Allowed"),
            StatusCode::NotAcceptable => (406, "Not Acceptable"),
            StatusCode::RequestTimeout => (408, "Request Timeout"),
            StatusCode::Conflict => (409, "Conflict"),
            StatusCode::ContentTooLarge => (413, "Content Too Large"),
            StatusCode::UnsupportedMediaType => (415, "Unsupported Media Type"),
            StatusCode::UnprocessableContent => (422, "Unprocessable Content"),
            StatusCode::TooManyRequests => (429, "Too Many Requests"),
            StatusCode::InternalServerError => (500, "Internal Server Error"),
            StatusCode::NotImplemented => (501, "Not Implemented"),
            StatusCode::BadGateway => (502, "Bad Gateway"),
            StatusCode::ServiceUnavailable => (503, "Service Unavailable"),
            StatusCode::GatewayTimeout => (504, "Gateway Timeout"),
            StatusCode::HttpVersionNotSupported => (505, "HTTP Version Not Supported"),
        };
        self.writer
            .write_all(format!("HTTP/1.1 {code} {reason}\r\n").as_bytes())
            .await?;
        Ok(())
    }
    pub async fn send_response(mut self, response: Response) -> Result<(), HttpError> {
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

        self.write_status_line(status).await?;
        for (k, v) in headers.iter() {
            self.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        self.writer.write_all(b"\r\n").await?;
        if !self.head_only {
            match body {
                Body::Bytes(bytes) => self.writer.write_all(&bytes).await?,
                Body::Stream(stream) => self.write_stream(stream, chunked).await?,
            }
        }
        self.writer.flush().await?;
        Ok(())
    }

    async fn write_stream(&mut self, mut stream: BodyStream, chunked: bool) -> io::Result<()> {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if !chunked {
                self.writer.write_all(&chunk).await?;
            } else if !chunk.is_empty() {
                // Write: <size-in-hex>\r\n<data>\r\n
                self.writer
                    .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                    .await?;
                self.writer.write_all(&chunk).await?;
                self.writer.write_all(b"\r\n").await?;
            }
        }
        if chunked {
            self.writer.write_all(b"0\r\n\r\n").await?;
        }
        Ok(())
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
    async fn should_set_powered_by_header(){       
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

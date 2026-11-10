use std::io;

use crate::{
    headers::{Cookie, Headers},
    request::{self, ParamError},
};
use futures::{Stream, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use yam_shared::IntoChunk;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusCode {
    Ok = 200,
    Created = 201,
    Accepted = 202,
    NoContent = 204,
    MovedPermanently = 301,
    Found = 302,
    SeeOther = 303,
    NotModified = 304,
    TemporaryRedirect = 307,
    PermanentRedirect = 308,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    NotAcceptable = 406,
    RequestTimeout = 408,
    Conflict = 409,
    ContentTooLarge = 413,
    UnsupportedMediaType = 415,
    UnprocessableContent = 422,
    TooManyRequests = 429,
    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
    GatewayTimeout = 504,
    HttpVersionNotSupported = 505,
}

#[derive(Debug)]
/// An HTTP response returned by a handler.
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Vec<u8>,
}
impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}
impl Response {
    /// Creates a `200 OK` plain-text response with an empty body.
    ///
    /// ```
    /// use yam_server::{Response, StatusCode};
    /// let response = Response::new();
    /// assert_eq!(response.status, StatusCode::Ok);
    /// ```
    pub fn new() -> Self {
        let mut headers = Headers::new();
        headers.set("connection", "keep-alive".to_string());
        headers.set("content-type", "text/plain".to_string());
        Self {
            status: StatusCode::Ok,
            headers,
            body: Vec::new(),
        }
    }
    /// Sets the response status.
    ///
    /// ```
    /// use yam_server::{Response, StatusCode};
    /// let response = Response::new().status(StatusCode::Created);
    /// assert_eq!(response.status, StatusCode::Created);
    /// ```
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
    /// Sets a response header, replacing existing values with the same name.
    ///
    /// ```
    /// use yam_server::Response;
    /// let response = Response::new().set("cache-control", "no-store");
    /// assert_eq!(response.headers.get("cache-control"), Some("no-store"));
    /// ```
    pub fn set(mut self, key: &str, value: &str) -> Self {
        self.headers.set(key, value.to_string());
        self
    }
    /// Sets the raw response body.
    ///
    /// ```
    /// use yam_server::Response;
    /// let response = Response::new().send("Hello world");
    /// assert_eq!(response.body, b"Hello world");
    /// ```
    pub fn send(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
    /// Serializes a value as JSON and sets the JSON content type.
    ///
    /// ```
    /// use serde_json::json;
    /// use yam_server::Response;
    /// let response = Response::new().json(&json!({ "ok": true }));
    /// assert_eq!(response.headers.get("content-type"), Some("application/json"));
    /// ```
    pub fn json(self, body: &impl Serialize) -> Self {
        match serde_json::to_vec(body) {
            Ok(body) => self.set("content-type", "application/json").send(body),
            Err(err) => Response::new()
                .status(StatusCode::InternalServerError)
                .send(format!("failed to serialize response: {err}")),
        }
    }
    /// Appends a `Set-Cookie` header to the response.
    ///
    /// ```
    /// use yam_server::{Cookie, Response};
    /// let response = Response::new().cookie(Cookie::new("session", "abc123"));
    /// assert!(response.headers.get("set-cookie").is_some());
    /// ```
    pub fn cookie(mut self, cookie: Cookie) -> Self {
        self.headers.append("set-cookie", cookie.to_string());
        self
    }
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl<T: IntoResponse, E: IntoResponse> IntoResponse for Result<T, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(r) => r.into_response(),
            Err(e) => e.into_response(),
        }
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::new().send(self)
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::new().send(self)
    }
}

impl IntoResponse for serde_json::Value {
    fn into_response(self) -> Response {
        Response::new()
            .set("content-type", "application/json")
            .send(self.to_string())
    }
}
/// Serializes the wrapped value as a JSON response body.
///
/// ```
/// use serde::Serialize;
/// use yam_server::{HttpError, IntoResponse, Json, StatusCode};
///
/// #[derive(Serialize)]
/// struct User { id: u32 }
///
/// fn get_users() -> Result<Json<Vec<User>>, HttpError> {
///     Ok(Json(vec![User { id: 1 }]))
/// }
///
/// let response = get_users().into_response();
/// assert_eq!(response.status, StatusCode::Ok);
/// assert_eq!(response.body, br#"[{"id":1}]"#);
/// ```
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        Response::new()
            .set("content-type", "application/json")
            .json(&self.0)
    }
}

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
    pub async fn send_response(self, mut response: Response) -> Result<(), HttpError> {
        let mut this = self;
        response
            .headers
            .set("content-length", response.body.len().to_string());
        this.write_status_line(response.status).await?;
        for (k, v) in response.headers.iter() {
            this.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        this.writer.write_all(b"\r\n").await?;
        if !this.head_only {
            this.writer.write_all(&response.body).await?;
        }
        this.writer.flush().await?;
        Ok(())
    }
    pub async fn send(self, body: impl AsRef<[u8]>) -> Result<(), HttpError> {
        self.send_response(Response {
            body: body.as_ref().to_vec(),
            ..Response::new()
        })
        .await
    }
    pub async fn json(self, body: &impl Serialize) -> Result<(), HttpError> {
        let response = Response::new().json(body);
        self.send_response(response).await
    }
    pub async fn create_stream_response<S, T>(
        mut self,
        mut response: Response,
        stream: S,
    ) -> Result<(), HttpError>
    where
        S: Stream<Item = T> + Unpin,
        T: IntoChunk,
    {
        response
            .headers
            .set("transfer-encoding", "chunked".to_string());
        self.write_status_line(response.status).await?;
        for (k, v) in response.headers.iter() {
            self.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        self.writer.write_all(b"\r\n").await?;
        let mut stream = stream;
        while let Some(chunk) = stream.next().await {
            match chunk.into_chunk() {
                Ok(chunk) if !chunk.is_empty() => {
                    // Write: <size-in-hex>\r\n<data>\r\n
                    self.writer
                        .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                        .await?;
                    self.writer.write_all(&chunk).await?;
                    self.writer.write_all(b"\r\n").await?;
                }
                Ok(_) => {} // Skip empty chunks
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    break;
                }
            }
        }

        // Write terminating chunk: 0\r\n\r\n
        self.writer.write_all(b"0\r\n\r\n").await?;
        self.writer.flush().await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("Request error: {0}")]
    Request(#[from] request::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid JSON body: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid form body: {0}")]
    Form(#[from] serde_urlencoded::de::Error),

    #[error("Invalid UTF-8 body: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("{0}")]
    Param(#[from] ParamError),
}

impl HttpError {
    pub fn status(&self) -> StatusCode {
        match self {
            HttpError::Io(_) => StatusCode::InternalServerError,
            HttpError::Request(request::Error::RequestTooLarge) => {
                StatusCode::ContentTooLarge
            }
            HttpError::Request(request::Error::MethodNotAllowed) => {
                StatusCode::NotImplemented
            }
            HttpError::Request(request::Error::Parse(
                request::ParseError::UnsupportedHttpVersion,
            )) => StatusCode::HttpVersionNotSupported,
            HttpError::Json(_)
            | HttpError::Form(_)
            | HttpError::Utf8(_)
            | HttpError::Request(_)
            | HttpError::Param(_) => StatusCode::BadRequest,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        Response::new().status(self.status()).send(self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .create_stream_response(
                Response::new(),
                futures::stream::iter([b"hello".to_vec(), b" world".to_vec()]),
            )
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("transfer-encoding: chunked\r\n"));
        assert!(!output.contains("content-length:"));
        assert!(output.ends_with("\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"));
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

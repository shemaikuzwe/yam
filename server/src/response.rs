use std::io;

use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    cookie::Cookie,
    headers::Headers,
    request::{self, ParamError},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusCode {
    StatusOk = 200,
    StatusCreated = 201,
    StatusAccepted = 202,
    StatusNoContent = 204,
    StatusMovedPermanently = 301,
    StatusFound = 302,
    StatusSeeOther = 303,
    StatusNotModified = 304,
    StatusTemporaryRedirect = 307,
    StatusPermanentRedirect = 308,
    StatusBadRequest = 400,
    StatusUnauthorized = 401,
    StatusForbidden = 403,
    StatusNotFound = 404,
    StatusMethodNotAllowed = 405,
    StatusNotAcceptable = 406,
    StatusRequestTimeout = 408,
    StatusConflict = 409,
    StatusContentTooLarge = 413,
    StatusUnsupportedMediaType = 415,
    StatusUnprocessableContent = 422,
    StatusTooManyRequests = 429,
    StatusInternalServerError = 500,
    StatusNotImplemented = 501,
    StatusBadGateway = 502,
    StatusServiceUnavailable = 503,
    StatusGatewayTimeout = 504,
    StatusHttpVersionNotSupported = 505,
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
    /// assert_eq!(response.status, StatusCode::StatusOk);
    /// ```
    pub fn new() -> Self {
        let mut headers = Headers::new();
        headers.set("connection", "close".to_string());
        headers.set("content-type", "text/plain".to_string());
        Self {
            status: StatusCode::StatusOk,
            headers,
            body: Vec::new(),
        }
    }
    /// Sets the response status.
    ///
    /// ```
    /// use yam_server::{Response, StatusCode};
    /// let response = Response::new().status(StatusCode::StatusCreated);
    /// assert_eq!(response.status, StatusCode::StatusCreated);
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
                .status(StatusCode::StatusInternalServerError)
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

#[derive(Debug)]
pub struct ResponseWriter<W: AsyncWrite> {
    writer: W,
}

impl<W: AsyncWrite + Unpin> ResponseWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
    async fn write_status_line(&mut self, status: StatusCode) -> io::Result<()> {
        let (code, reason) = match status {
            StatusCode::StatusOk => (200, "OK"),
            StatusCode::StatusCreated => (201, "Created"),
            StatusCode::StatusAccepted => (202, "Accepted"),
            StatusCode::StatusNoContent => (204, "No Content"),
            StatusCode::StatusMovedPermanently => (301, "Moved Permanently"),
            StatusCode::StatusFound => (302, "Found"),
            StatusCode::StatusSeeOther => (303, "See Other"),
            StatusCode::StatusNotModified => (304, "Not Modified"),
            StatusCode::StatusTemporaryRedirect => (307, "Temporary Redirect"),
            StatusCode::StatusPermanentRedirect => (308, "Permanent Redirect"),
            StatusCode::StatusBadRequest => (400, "Bad Request"),
            StatusCode::StatusUnauthorized => (401, "Unauthorized"),
            StatusCode::StatusForbidden => (403, "Forbidden"),
            StatusCode::StatusNotFound => (404, "Not Found"),
            StatusCode::StatusMethodNotAllowed => (405, "Method Not Allowed"),
            StatusCode::StatusNotAcceptable => (406, "Not Acceptable"),
            StatusCode::StatusRequestTimeout => (408, "Request Timeout"),
            StatusCode::StatusConflict => (409, "Conflict"),
            StatusCode::StatusContentTooLarge => (413, "Content Too Large"),
            StatusCode::StatusUnsupportedMediaType => (415, "Unsupported Media Type"),
            StatusCode::StatusUnprocessableContent => (422, "Unprocessable Content"),
            StatusCode::StatusTooManyRequests => (429, "Too Many Requests"),
            StatusCode::StatusInternalServerError => (500, "Internal Server Error"),
            StatusCode::StatusNotImplemented => (501, "Not Implemented"),
            StatusCode::StatusBadGateway => (502, "Bad Gateway"),
            StatusCode::StatusServiceUnavailable => (503, "Service Unavailable"),
            StatusCode::StatusGatewayTimeout => (504, "Gateway Timeout"),
            StatusCode::StatusHttpVersionNotSupported => (505, "HTTP Version Not Supported"),
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
        this.writer.write_all(&response.body).await?;
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
            HttpError::Io(_) => StatusCode::StatusInternalServerError,
            HttpError::Request(request::Error::RequestTooLarge) => {
                StatusCode::StatusContentTooLarge
            }
            HttpError::Request(request::Error::MethodNotAllowed) => {
                StatusCode::StatusNotImplemented
            }
            HttpError::Request(request::Error::Parse(
                request::ParseError::UnsupportedHttpVersion,
            )) => StatusCode::StatusHttpVersionNotSupported,
            HttpError::Json(_)
            | HttpError::Form(_)
            | HttpError::Utf8(_)
            | HttpError::Request(_)
            | HttpError::Param(_) => StatusCode::StatusBadRequest,
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
                    .status(StatusCode::StatusOk)
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

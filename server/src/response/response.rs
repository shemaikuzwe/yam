use std::{fmt, io, pin::Pin};

use crate::{
    headers::{Cookie, Headers},
    request::{self, ParamError},
};
use futures::{Stream, StreamExt};
use serde::Serialize;
use thiserror::Error;
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

pub type BodyStream = Pin<Box<dyn Stream<Item = io::Result<Vec<u8>>> + Send>>;

/// A response body: buffered bytes, or a stream written as it is produced.
pub enum Body {
    Bytes(Vec<u8>),
    Stream(BodyStream),
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Bytes(bytes) => f.debug_tuple("Bytes").field(bytes).finish(),
            Body::Stream(_) => f.write_str("Stream(..)"),
        }
    }
}

/// Compares buffered bodies with bytes; a stream never equals bytes.
impl<T> PartialEq<T> for Body
where
    Vec<u8>: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        matches!(self, Body::Bytes(bytes) if bytes == other)
    }
}

#[derive(Debug)]
/// An HTTP response returned by a handler.
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Body,
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
        headers.set("x-powered-by", "Yam".into());
        Self {
            status: StatusCode::Ok,
            headers,
            body: Body::Bytes(Vec::new()),
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
        self.body = Body::Bytes(body.into());
        self
    }
    /// Sets a streamed body, written chunk by chunk without buffering it all.
    ///
    /// Uses `transfer-encoding: chunked` unless a `content-length` header is set.
    ///
    /// ```
    /// use yam_server::Response;
    /// let response = Response::new().stream_response(futures::stream::iter(["hello", " world"]));
    /// assert_ne!(response.body, b"hello world");
    /// ```
    pub fn stream_response<S, T>(mut self, stream: S) -> Self
    where
        S: Stream<Item = T> + Send + 'static,
        T: IntoChunk + 'static,
    {
        self.body = Body::Stream(Box::pin(stream.map(IntoChunk::into_chunk)));
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
    /// Creates a `302 Found` redirect to `location`.
    /// Use `.status(...)` to change it, e.g. `StatusCode::SeeOther` after a POST.
    ///
    /// ```
    /// use yam_server::{Response, StatusCode};
    /// let response = Response::redirect("/login");
    /// assert_eq!(response.status, StatusCode::Found);
    /// assert_eq!(response.headers.get("location"), Some("/login"));
    ///
    /// // after a form POST
    /// let response = Response::redirect("/users/1").status(StatusCode::SeeOther);
    /// assert_eq!(response.status, StatusCode::SeeOther);
    /// ```
    pub fn redirect(location: &str) -> Self {
        Response::new()
            .status(StatusCode::Found)
            .set("location", location)
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
            HttpError::Request(request::Error::RequestTooLarge) => StatusCode::ContentTooLarge,
            HttpError::Request(request::Error::MethodNotAllowed) => StatusCode::NotImplemented,
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

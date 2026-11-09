use std::{io, str::Utf8Error};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("relative URL requires a base URL")]
    MissingBaseUrl,

    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
    #[error("invalid hostname: {0}")]
    InvalidHostname(String),
    #[error("missing port: {0}")]
    MissingPort(String),
    #[error("invalid HTTP reponse: {0}")]
    InvalidResponse(String),
    #[error("invalid JSON body: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid form body: {0}")]
    Form(#[from] serde_urlencoded::ser::Error),

    #[error("response body is not valid UTF-8: {0}")]
    Utf8(#[from] Utf8Error),
    #[error("request timed out")]
    Timeout,
}

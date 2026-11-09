use std::io;
use thiserror::Error;
use yam_shared::HeaderParseError;

#[derive(Debug, PartialEq, Error)]
pub enum ParseError {
    #[error("incomplete request line")]
    IncompleteRequestLine,
    #[error("invalid request line")]
    InvalidRequestLine,
    #[error("unsupported http version")]
    UnsupportedHttpVersion,
    #[error("incomplete field line")]
    IncompleteFieldLine,
    #[error("invalid field line")]
    InvalidFieldLine,
    #[error("invalid header key")]
    InvalidHeaderKey,
    #[error("invalid content-length header")]
    InvalidContentLengthHeader,
    #[error("request too large")]
    RequestTooLarge,
    #[error("incomplete chunk size")]
    IncompleteChunkSize,
    #[error("invalid chunk size")]
    InvalidChunkSize,
    #[error("invalid chunked encoding")]
    InvalidChunkedEncoding,
}

impl From<HeaderParseError> for ParseError {
    fn from(error: HeaderParseError) -> Self {
        match error {
            HeaderParseError::IncompleteFieldLine => Self::IncompleteFieldLine,
            HeaderParseError::InvalidFieldLine => Self::InvalidFieldLine,
            HeaderParseError::InvalidHeaderKey => Self::InvalidHeaderKey,
        }
    }
}

impl From<yam_shared::ChunkError> for ParseError {
    fn from(error: yam_shared::ChunkError) -> Self {
        match error {
            yam_shared::ChunkError::IncompleteChunkSize => Self::IncompleteChunkSize,
            yam_shared::ChunkError::InvalidChunkSize => Self::InvalidChunkSize,
            yam_shared::ChunkError::InvalidChunkedEncoding => Self::InvalidChunkedEncoding,
            yam_shared::ChunkError::TooLarge => Self::RequestTooLarge,
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("request too large")]
    RequestTooLarge,
    #[error("unexpected end of input")]
    UnexexpectedEndOfInput,
    #[error("connection closed before the next request")]
    GracefulEof,
    #[error("method not allowed")]
    MethodNotAllowed,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParamError {
    #[error("missing param: {0}")]
    Missing(String),
    #[error("invalid typed param: {0}")]
    Invalid(String),
}

use std::io;

use futures::{Stream, StreamExt, stream::BoxStream};
use serde::Serialize;
use yam_shared::IntoChunk;

pub(crate) type ChunkStream = BoxStream<'static, io::Result<Vec<u8>>>;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PATCH,
    PUT,
    DELETE,
    OPTIONS,
}

impl Method {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::HEAD => "HEAD",
            Self::POST => "POST",
            Self::PATCH => "PATCH",
            Self::PUT => "PUT",
            Self::DELETE => "DELETE",
            Self::OPTIONS => "OPTIONS",
        }
    }
}

pub struct Body {
    bytes: Vec<u8>,
    stream: Option<ChunkStream>,
    content_type: Option<String>,
}

impl Body {
    /// Serializes a value as an `application/json` body.
    ///
    /// ```
    /// use serde_json::json;
    /// use yam_client::Body;
    /// let body = Body::json(&json!({ "name": "Yam" }))?;
    /// # let _ = body;
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self {
            bytes: serde_json::to_vec(value)?,
            stream: None,
            content_type: Some("application/json".into()),
        })
    }

    /// Serializes a value as an `application/x-www-form-urlencoded` body.
    ///
    /// ```
    /// use serde::Serialize;
    /// use yam_client::Body;
    /// #[derive(Serialize)]
    /// struct Login<'a> { email: &'a str }
    /// let body = Body::form(&Login { email: "user@example.com" })?;
    /// # let _ = body;
    /// # Ok::<(), serde_urlencoded::ser::Error>(())
    /// ```
    pub fn form<T: Serialize>(value: &T) -> Result<Self, serde_urlencoded::ser::Error> {
        Ok(Self {
            bytes: serde_urlencoded::to_string(value)?.into_bytes(),
            stream: None,
            content_type: Some("application/x-www-form-urlencoded".into()),
        })
    }

    /// Creates a UTF-8 plain-text body.
    ///
    /// ```
    /// use yam_client::Body;
    /// let body = Body::text("Hello world");
    /// ```
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            bytes: value.into().into_bytes(),
            stream: None,
            content_type: Some("text/plain; charset=utf-8".into()),
        }
    }

    /// Creates a raw byte body without inferring a content type.
    ///
    /// ```
    /// use yam_client::Body;
    /// let body = Body::bytes([0, 1, 2]);
    /// ```
    pub fn bytes(value: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: value.into(),
            stream: None,
            content_type: None,
        }
    }

    /// Creates a body from a stream of chunks, sent with `transfer-encoding: chunked`.
    ///
    /// ```
    /// use futures::stream;
    /// use yam_client::Body;
    /// let body = Body::stream(stream::iter([b"hello".to_vec(), b" world".to_vec()]));
    /// # let _ = body;
    /// ```
    pub fn stream<S, T>(stream: S) -> Self
    where
        S: Stream<Item = T> + Send + 'static,
        T: IntoChunk + Send + 'static,
    {
        Self {
            bytes: Vec::new(),
            stream: Some(Box::pin(stream.map(|chunk| chunk.into_chunk()))),
            content_type: None,
        }
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, Option<ChunkStream>, Option<String>) {
        (self.bytes, self.stream, self.content_type)
    }
}

#[derive(Default)]
pub struct RequestOptions {
    pub headers: Vec<(String, String)>,
    pub body: Option<Body>,
}

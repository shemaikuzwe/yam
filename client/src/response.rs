use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt};
use yam_shared::chunked::parse_chunked_body;

use crate::{compression, error::Error};

const SEPARATOR: &[u8] = b"\r\n\r\n";

/// An HTTP response. Response bodies with supported `content-encoding` values
/// (such as `gzip` and `deflate`) are automatically decompressed.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// Deserializes the response body as JSON.
    ///
    /// ```
    /// use serde::Deserialize;
    /// use yam_client::Response;
    /// #[derive(Deserialize)]
    /// struct User { id: u64 }
    ///
    /// let response = Response {
    ///     status: 200,
    ///     headers: vec![],
    ///     body: br#"{"id":42}"#.to_vec(),
    /// };
    /// let user: User = response.json()?;
    /// assert_eq!(user.id, 42);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn json<T>(&self) -> Result<T, serde_json::Error>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body)
    }
    /// Reads the response body as UTF-8 text.
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }
    /// Returns `true` for status codes from 200 through 299.
    ///
    /// ```
    /// use yam_client::Response;
    /// let response = Response { status: 204, headers: vec![], body: vec![] };
    /// assert!(response.ok());
    /// ```
    pub fn ok(&self) -> bool {
        (200..=299).contains(&self.status)
    }
    /// Returns the raw response body.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
    /// Returns the first matching header value.
    ///
    /// ```
    /// use yam_client::Response;
    /// let response = Response {
    ///     status: 200,
    ///     headers: vec![("content-type".into(), "application/json".into())],
    ///     body: vec![],
    /// };
    /// assert_eq!(response.header("Content-Type"), Some("application/json"));
    /// ```
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl Response {
    pub(crate) async fn read<S>(stream: &mut S) -> Result<(Response, bool), Error>
    where
        S: AsyncRead + Unpin,
    {
        let mut buffer = Vec::new();
        let idx = loop {
            if let Some(idx) = buffer
                .windows(SEPARATOR.len())
                .position(|window| window == SEPARATOR)
            {
                break idx;
            }
            if stream.read_buf(&mut buffer).await? == 0 {
                return Err(Error::InvalidResponse(
                    "response is missing header separator".into(),
                ));
            }
        };

        let (status, mut headers) = parse_head(&buffer[..idx])?;
        let mut keep_alive = !headers
            .iter()
            .any(|(name, value)| name == "connection" && value.eq_ignore_ascii_case("close"));
        let mut body = buffer[idx + SEPARATOR.len()..].to_vec();

        if has_body(status) {
            if is_chunked(&headers) {
                body = read_chunked(stream, body).await?;
            } else {
                match headers
                    .iter()
                    .find(|(name, _)| name == "content-length")
                    .map(|(_, value)| value)
                {
                    Some(value) => {
                        let length = value.parse::<usize>().map_err(|_| {
                            Error::InvalidResponse("response has invalid content-length".into())
                        })?;
                        let read = body.len();
                        if read < length {
                            body.resize(length, 0);
                            stream.read_exact(&mut body[read..]).await?;
                        } else {
                            body.truncate(length);
                        }
                    }
                    None => {
                        stream.read_to_end(&mut body).await?;
                        keep_alive = false;
                    }
                }
            }
        } else {
            body.clear();
        }

        let body = compression::decode(&mut headers, body)?;

        Ok((
            Response {
                status,
                headers,
                body,
            },
            keep_alive,
        ))
    }
}

/// Informational, no content, and not modified responses never carry a body.
fn has_body(status: u16) -> bool {
    !(matches!(status, 204 | 304) || (100..200).contains(&status))
}

fn is_chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
    })
}

async fn read_chunked<S>(stream: &mut S, mut raw: Vec<u8>) -> Result<Vec<u8>, Error>
where
    S: AsyncRead + Unpin,
{
    let mut body = Vec::new();
    loop {
        let (consumed, done) = parse_chunked_body(&raw, &mut body, usize::MAX)
            .map_err(|e| Error::InvalidResponse(e.to_string()))?;
        if done {
            return Ok(body);
        }
        raw.drain(..consumed);
        if stream.read_buf(&mut raw).await? == 0 {
            return Err(Error::InvalidResponse(
                "chunked response ended before the terminal chunk".into(),
            ));
        }
    }
}

fn parse_head(head: &[u8]) -> Result<(u16, Vec<(String, String)>), Error> {
    let head = std::str::from_utf8(head)
        .map_err(|_| Error::InvalidResponse("response head is not valid UTF-8".into()))?;
    let status_line = head
        .split("\r\n")
        .next()
        .ok_or_else(|| Error::InvalidResponse("response is missing status line".into()))?;

    let mut parts = status_line.split_whitespace();

    let _ = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse("response is missing HTTP version".into()))?;

    let status = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse("response is missing status code".into()))?
        .parse::<u16>()
        .map_err(|_| Error::InvalidResponse("response has invalid status code".into()))?;

    let mut headers = Vec::new();
    for line in head.split("\r\n").skip(1) {
        let (name, value) = line
            .split_once(":")
            .ok_or_else(|| Error::InvalidResponse(format!("invalid response header: {line}")))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    Ok((status, headers))
}

use std::{format, io, matches, str::Utf8Error, time::Duration};

use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

#[derive(Debug, Default)]
pub struct HttpClient {
    base_url: Option<String>,
    timeout: Option<Duration>,
    // headers:
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json<T>(&self) -> Result<T, serde_json::Error>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body)
    }
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }
    pub fn ok(&self) -> bool {
        (200..=299).contains(&self.status)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum Method {
    GET,
    POST,
    PATCH,
    PUT,
    DELETE,
}
impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::POST => "POST",
            Self::PATCH => "PATCH",
            Self::PUT => "PUT",
            Self::DELETE => "DELETE",
        }
    }
}

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

    #[error("response body is not valid UTF-8: {0}")]
    Utf8(#[from] Utf8Error),
    #[error("request timed out")]
    Timeout,
}

macro_rules! route_verb {
    // Methods without a request body.
    ($(#[$docs:meta])* $name:ident => $method:ident) => {
        $(#[$docs])*
        pub async fn $name(
            &self,
            path: &str,
        ) -> Result<Response, Error> {
            self.request(Method::$method, path, &[]).await
        }
    };

    // Methods with a request body.
    ($(#[$docs:meta])* $name:ident => $method:ident, body) => {
        $(#[$docs])*
        pub async fn $name(
            &self,
            path: &str,
            body: impl AsRef<[u8]>,
        ) -> Result<Response, Error> {
            self.request(Method::$method, path, body.as_ref()).await
        }
    };
}

#[derive(Default)]
pub struct HttpClientConfig {
    pub base_url: Option<String>,
    pub timeout: Option<Duration>,
}
impl HttpClient {
    pub fn new(config: HttpClientConfig) -> Self {
        Self {
            base_url: config.base_url,
            timeout: config.timeout,
        }
    }
    route_verb!(get=>GET);
    route_verb!(post=>POST,body);
    route_verb!(put=>PUT,body);
    route_verb!(patch=>PATCH,body);
    route_verb!(delete=>DELETE);

    async fn request(&self, method: Method, path: &str, body: &[u8]) -> Result<Response, Error> {
        let request = self.run_request(method, path, body);
        match self.timeout {
            Some(duration) => tokio::time::timeout(duration, request)
                .await
                .map_err(|_| Error::Timeout)?,
            None => request.await,
        }
    }

    async fn run_request(
        &self,
        method: Method,
        path: &str,
        body: &[u8],
    ) -> Result<Response, Error> {
        let url = self.resolve_url(path)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::UnsupportedScheme(url.scheme().into()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| Error::InvalidHostname(url.to_string()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| Error::MissingPort(url.to_string()))?;

        let mut stream = TcpStream::connect((host, port)).await?;
        let mut request_target = url.path().to_string();
        if let Some(query) = url.query() {
            request_target.push('?');
            request_target.push_str(query);
        }

        let request_head = format!(
            concat!(
                "{} {} HTTP/1.1\r\n",
                "Host: {}:{}\r\n",
                "Connection: close\r\n",
                "Content-Length: {}\r\n",
                "\r\n",
            ),
            method.as_str(),
            request_target,
            host,
            port,
            body.len(),
        );

        stream.write_all(request_head.as_bytes()).await?;
        stream.write_all(body).await?;
        stream.flush().await?;
        let mut response_bytes = Vec::new();
        stream.read_to_end(&mut response_bytes).await?;
        let separator = b"\r\n\r\n";
        let idx = response_bytes
            .windows(separator.len())
            .position(|window| window == separator)
            .ok_or_else(|| Error::InvalidResponse("response is missing header separator".into()))?;
        let response_head = &response_bytes[..idx];
        let response_body = &response_bytes[idx + separator.len()..];
        let response_head = std::str::from_utf8(response_head)
            .map_err(|_| Error::InvalidResponse("response head is not valid UTF-8".into()))?;
        let status_line = response_head
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
        for line in response_head.split("\r\n").skip(1) {
            let (name, value) = line.split_once(":").ok_or_else(|| {
                Error::InvalidResponse(format!("invalid response header: {line}"))
            })?;
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
        Ok(Response {
            status,
            headers,
            body: response_body.into(),
        })
    }
    fn resolve_url(&self, value: &str) -> Result<Url, Error> {
        if let Ok(url) = Url::parse(value) {
            return Ok(url);
        }
        let base_url = self
            .base_url
            .as_ref()
            .ok_or_else(|| Error::MissingBaseUrl)?;
        let url = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            value.trim_start_matches('/')
        );
        Url::parse(&url).map_err(Error::Url)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::net::TcpListener;

    use super::{Error, HttpClient, HttpClientConfig};

    fn client() -> HttpClient {
        HttpClient::new(HttpClientConfig {
            base_url: Some("http://localhost:3000/api/v1".into()),
            ..Default::default()
        })
    }

    #[test]
    fn should_resolve_root_relative_to_base_url() {
        let url = client().resolve_url("/").expect("URL should resolve");

        assert_eq!(url.as_str(), "http://localhost:3000/api/v1/");
    }

    #[test]
    fn should_resolve_path_relative_to_base_url() {
        let url = client().resolve_url("/users").expect("URL should resolve");

        assert_eq!(url.as_str(), "http://localhost:3000/api/v1/users");
    }

    #[test]
    fn should_preserve_absolute_url() {
        let url = client()
            .resolve_url("http://example.com/users")
            .expect("URL should resolve");

        assert_eq!(url.as_str(), "http://example.com/users");
    }

    #[tokio::test]
    async fn should_timeout_request() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("server should accept");
            std::future::pending::<()>().await;
        });
        let client = HttpClient::new(HttpClientConfig {
            base_url: Some(format!("http://{address}")),
            timeout: Some(Duration::from_millis(50)),
        });

        let result = client.get("/").await;

        server.abort();
        assert!(matches!(result, Err(Error::Timeout)));
    }
}

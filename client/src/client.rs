use std::{format, io, str::Utf8Error, sync::Arc, time::Duration};

use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use url::Url;
use yam_shared::Headers;

#[derive(Debug)]
pub struct HttpClient {
    base_url: Option<String>,
    timeout: Option<Duration>,
    tls_configuration: Arc<ClientConfig>,
    headers: Headers,
}
impl Default for HttpClient {
    fn default() -> Self {
        Self::new(HttpClientConfig::default())
    }
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
    #[error("invalid form body: {0}")]
    Form(#[from] serde_urlencoded::ser::Error),

    #[error("response body is not valid UTF-8: {0}")]
    Utf8(#[from] Utf8Error),
    #[error("request timed out")]
    Timeout,
}

#[derive(Debug)]
pub struct Body {
    bytes: Vec<u8>,
    content_type: Option<String>,
}

impl Body {
    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self {
            bytes: serde_json::to_vec(value)?,
            content_type: Some("application/json".into()),
        })
    }

    pub fn form<T: Serialize>(value: &T) -> Result<Self, serde_urlencoded::ser::Error> {
        Ok(Self {
            bytes: serde_urlencoded::to_string(value)?.into_bytes(),
            content_type: Some("application/x-www-form-urlencoded".into()),
        })
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self {
            bytes: value.into().into_bytes(),
            content_type: Some("text/plain; charset=utf-8".into()),
        }
    }

    pub fn bytes(value: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: value.into(),
            content_type: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct RequestOptions {
    pub headers: Vec<(String, String)>,
    pub body: Option<Body>,
}

macro_rules! route_verb {
    ($(#[$docs:meta])* $name:ident => $method:ident) => {
        $(#[$docs])*
        pub async fn $name(
            &self,
            path: &str,
            options: RequestOptions,
        ) -> Result<Response, Error> {
            self.request(Method::$method, path, options).await
        }
    };
}

#[derive(Default)]
pub struct HttpClientConfig {
    pub base_url: Option<String>,
    pub timeout: Option<Duration>,
    pub tls_configuration: Option<ClientConfig>,
    pub headers: Vec<(String, String)>,
}
impl HttpClient {
    pub fn new(config: HttpClientConfig) -> Self {
        let tls_configuration = config.tls_configuration.unwrap_or_else(|| {
            let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth()
        });
        let mut headers = Headers::new();
        for (name, value) in config.headers {
            headers.append(&name, value);
        }
        Self {
            base_url: config.base_url,
            timeout: config.timeout,
            tls_configuration: Arc::new(tls_configuration),
            headers,
        }
    }
    route_verb!(get=>GET);
    route_verb!(post=>POST);
    route_verb!(put=>PUT);
    route_verb!(patch=>PATCH);
    route_verb!(delete=>DELETE);

    async fn request(
        &self,
        method: Method,
        path: &str,
        options: RequestOptions,
    ) -> Result<Response, Error> {
        let request = self.run_request(method, path, options);
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
        options: RequestOptions,
    ) -> Result<Response, Error> {
        let url = self.resolve_url(path)?;
        let host = url
            .host_str()
            .ok_or_else(|| Error::InvalidHostname(url.to_string()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| Error::MissingPort(url.to_string()))?;

        let stream = TcpStream::connect((host, port)).await?;
        let mut request_target = url.path().to_string();
        if let Some(query) = url.query() {
            request_target.push('?');
            request_target.push_str(query);
        }

        let mut headers = self.headers.clone();
        let body = options.body.unwrap_or_else(|| Body::bytes(Vec::new()));
        if let Some(content_type) = body.content_type {
            headers.set("content-type", content_type);
        }
        for (name, value) in options.headers {
            headers.set(&name, value);
        }
        headers.set("host", format!("{host}:{port}"));
        headers.set("connection", "close".into());
        headers.set("content-length", body.bytes.len().to_string());

        let mut request_head = format!("{} {} HTTP/1.1\r\n", method.as_str(), request_target);
        for (name, value) in headers.iter() {
            request_head.push_str(&format!("{name}: {value}\r\n"));
        }
        request_head.push_str("\r\n");
        let response_bytes = match url.scheme() {
            "http" => Self::send_request(stream, request_head.as_bytes(), &body.bytes).await?,
            "https" => {
                let server_name = ServerName::try_from(host.to_owned())
                    .map_err(|_| Error::InvalidHostname(host.into()))?;
                let connector = TlsConnector::from(Arc::clone(&self.tls_configuration));
                let tls_stream = connector.connect(server_name, stream).await?;
                Self::send_request(tls_stream, request_head.as_bytes(), &body.bytes).await?
            }
            scheme => {
                return Err(Error::UnsupportedScheme(scheme.into()));
            }
        };

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
    async fn send_request<S>(
        mut stream: S,
        request_head: &[u8],
        body: &[u8],
    ) -> Result<Vec<u8>, Error>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        stream.write_all(request_head).await?;
        stream.write_all(body).await?;
        stream.flush().await?;

        let mut response_bytes = Vec::new();
        stream.read_to_end(&mut response_bytes).await?;

        Ok(response_bytes)
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
    use super::{HttpClient, HttpClientConfig};

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
}

use std::{
    format,
    io::{self, Error},
    print, println,
    time::Duration,
    todo,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

#[derive(Debug, Default)]
pub struct HttpClient {
    base_url: Option<Url>,
    timeout: Option<Duration>,
    // headers:
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
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

impl HttpClient {
    pub fn new() {}
    route_verb!(get=>GET);
    route_verb!(post=>POST,body);
    route_verb!(put=>PUT,body);
    route_verb!(patch=>PATCH,body);
    route_verb!(delete=>DELETE);

    async fn request(&self, method: Method, path: &str, body: &[u8]) -> Result<Response, Error> {
        let url = self.resolve_url(path)?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported URL scheme: {}", url.scheme()),
            ));
        }
        let host = url.host_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "URL is missing hostname")
        })?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "URL is missing port"))?;
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
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "response is missing header separator",
                )
            })?;
        let response_head = &response_bytes[..idx];
        let response_body = &response_bytes[idx + separator.len()..];
        let response_head = std::str::from_utf8(response_head).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "response head is not valid UTF-8",
            )
        })?;
        let status_line = response_head.split("\r\n").next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "response is missing status line",
            )
        })?;

        let mut parts = status_line.split_whitespace();

        let _ = parts.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "response is missing HTTP version",
            )
        })?;

        let status = parts
            .next()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "response is missing status code",
                )
            })?
            .parse::<u16>()
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "response has invalid status code",
                )
            })?;
        let mut headers = Vec::new();
        for line in response_head.split("\r\n").skip(1) {
            let (name, value) = line.split_once(":").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid response header: {line}"),
                )
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
        let base_url = self.base_url.as_ref().ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "relative url requires base url",
        ))?;
        base_url
            .join(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

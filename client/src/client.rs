use std::{format, io::Write, sync::Arc, time::Duration};

use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use url::Url;
use yam_shared::Headers;

use crate::{
    error::Error,
    pool::Pools,
    request::{Body, ChunkStream, Method, RequestOptions},
    response::Response,
    transport,
};

#[derive(Debug)]
pub struct HttpClient {
    base_url: Option<String>,
    timeout: Option<Duration>,
    tls_configuration: Arc<ClientConfig>,
    headers: Headers,
    max_redirects: usize,
    pools: Pools,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new(HttpClientConfig::default())
    }
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

pub struct HttpClientConfig {
    /// Base URL used when a request is given a relative path.
    pub base_url: Option<String>,
    /// Maximum duration for the complete request and response exchange.
    pub timeout: Option<Duration>,
    /// TLS roots and client settings. Web PKI roots are used by default.
    pub tls_configuration: Option<ClientConfig>,
    /// Headers included with every request can be overridden per request.
    pub headers: Vec<(String, String)>,
    /// How many redirects to follow before returning the redirect response. `0` disables following.
    pub max_redirects: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            timeout: None,
            tls_configuration: None,
            headers: Vec::new(),
            max_redirects: 10,
        }
    }
}

impl HttpClient {
    /// Creates a client from reusable URL, timeout, TLS, and header defaults.
    ///
    /// ```
    /// use std::time::Duration;
    /// use yam_client::{HttpClient, HttpClientConfig};
    /// let client = HttpClient::new(HttpClientConfig {
    ///     base_url: Some("https://example.com/api".into()),
    ///     timeout: Some(Duration::from_secs(10)),
    ///     headers: vec![("accept".into(), "application/json".into())],
    ///     ..Default::default()
    /// });
    /// ```
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
            max_redirects: config.max_redirects,
            pools: Pools::default(),
        }
    }
    route_verb!(
        #[doc = r#"Sends a GET request.

```no_run
use yam_client::{Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let response = client
    .get("https://example.com/users/42", RequestOptions::default())
    .await?;
# let _ = response;
# Ok(())
# }
```"#]
        get => GET
    );
    route_verb!(
        #[doc = r#"Sends a HEAD request. The response has headers but an empty body.

```no_run
use yam_client::{Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let response = client
    .head("https://example.com/files/report.pdf", RequestOptions::default())
    .await?;
# let _ = response;
# Ok(())
# }
```"#]
        head => HEAD
    );
    route_verb!(
        #[doc = r#"Sends a POST request.

```no_run
use serde_json::json;
use yam_client::{Body, Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let options = RequestOptions {
    body: Some(Body::json(&json!({ "name": "Yam" }))?),
    ..Default::default()
};
let response = client.post("https://example.com/users", options).await?;
# let _ = response;
# Ok(())
# }
```"#]
        post => POST
    );
    route_verb!(
        #[doc = r#"Sends a PUT request.

```no_run
use serde_json::json;
use yam_client::{Body, Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let options = RequestOptions {
    body: Some(Body::json(&json!({ "name": "New name" }))?),
    ..Default::default()
};
let response = client.put("https://example.com/users/42", options).await?;
# let _ = response;
# Ok(())
# }
```"#]
        put => PUT
    );
    route_verb!(
        #[doc = r#"Sends a PATCH request.

```no_run
use serde_json::json;
use yam_client::{Body, Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let options = RequestOptions {
    body: Some(Body::json(&json!({ "active": true }))?),
    ..Default::default()
};
let response = client.patch("https://example.com/users/42", options).await?;
# let _ = response;
# Ok(())
# }
```"#]
        patch => PATCH
    );
    route_verb!(
        #[doc = r#"Sends a DELETE request.

```no_run
use yam_client::{Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let response = client
    .delete("https://example.com/users/42", RequestOptions::default())
    .await?;
# let _ = response;
# Ok(())
# }
```"#]
        delete => DELETE
    );
    route_verb!(
        #[doc = r#"Sends an OPTIONS request. The `allow` header lists the methods the path supports.

```no_run
use yam_client::{Error, HttpClient, RequestOptions};

# async fn run() -> Result<(), Error> {
let client = HttpClient::default();
let response = client
    .options("https://example.com/users", RequestOptions::default())
    .await?;
# let _ = response.header("allow");
# Ok(())
# }
```"#]
        options => OPTIONS
    );

    async fn request(
        &self,
        method: Method,
        path: &str,
        options: RequestOptions,
    ) -> Result<Response, Error> {
        let request = self.follow_redirects(method, path, options);
        match self.timeout {
            Some(duration) => tokio::time::timeout(duration, request)
                .await
                .map_err(|_| Error::Timeout)?,
            None => request.await,
        }
    }

    /// Sends the request and follows up to `max_redirects` redirects.
    /// 301/302/303 switch to a bodyless GET; 307/308 resend the same method and body.
    async fn follow_redirects(
        &self,
        mut method: Method,
        path: &str,
        options: RequestOptions,
    ) -> Result<Response, Error> {
        let mut url = self.resolve_url(path)?;
        let mut headers = options.headers;
        let (mut body, mut chunks, mut content_type) = options
            .body
            .unwrap_or_else(|| Body::bytes(Vec::new()))
            .into_parts();
        let streamed = chunks.is_some();
        let origin = url.origin();
        let mut redirects = 0;
        loop {
            let response = self
                .run_request(
                    method,
                    &url,
                    &headers,
                    &body,
                    chunks.take(),
                    content_type.as_deref(),
                    url.origin() == origin,
                )
                .await?;

            let keeps_body = matches!(response.status, 307 | 308);
            let is_redirect = keeps_body || matches!(response.status, 301..=303);
            // A streamed body is consumed by the first send and can't be resent.
            if !is_redirect || redirects == self.max_redirects || (keeps_body && streamed) {
                return Ok(response);
            }
            let Some(location) = response.header("location") else {
                return Ok(response);
            };
            url = url.join(location)?;
            redirects += 1;

            if !keeps_body && !matches!(method, Method::GET | Method::HEAD) {
                method = Method::GET;
                body.clear();
                content_type = None;
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case("content-type"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_request(
        &self,
        method: Method,
        url: &Url,
        headers: &[(String, String)],
        body: &[u8],
        chunks: Option<ChunkStream>,
        content_type: Option<&str>,
        same_origin: bool,
    ) -> Result<Response, Error> {
        let host = url
            .host_str()
            .ok_or_else(|| Error::InvalidHostname(url.to_string()))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| Error::MissingPort(url.to_string()))?;

        let request_head = self.write_head(
            method,
            url,
            host,
            port,
            headers,
            content_type,
            chunks.is_none().then_some(body.len()),
            same_origin,
        );

        transport::send(
            &self.pools,
            url,
            &self.tls_configuration,
            method,
            request_head,
            body,
            chunks,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    fn write_head(
        &self,
        method: Method,
        url: &Url,
        host: &str,
        port: u16,
        request_headers: &[(String, String)],
        content_type: Option<&str>,
        content_length: Option<usize>,
        same_origin: bool,
    ) -> Vec<u8> {
        let reserved = |name: &str| {
            ["host", "connection", "content-length", "transfer-encoding"]
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
                // Credentials aren't forwarded when a redirect leaves the original origin.
                || (!same_origin
                    && ["authorization", "cookie", "proxy-authorization"]
                        .iter()
                        .any(|sensitive| name.eq_ignore_ascii_case(sensitive)))
        };
        fn push(head: &mut Vec<u8>, name: &str, value: &str) {
            head.extend(name.bytes().map(|byte| byte.to_ascii_lowercase()));
            head.extend_from_slice(b": ");
            head.extend_from_slice(value.as_bytes());
            head.extend_from_slice(b"\r\n");
        }
        let in_request = |name: &str| {
            request_headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name))
        };

        let mut head = Vec::with_capacity(256);
        head.extend_from_slice(method.as_str().as_bytes());
        head.push(b' ');
        head.extend_from_slice(url.path().as_bytes());
        if let Some(query) = url.query() {
            head.push(b'?');
            head.extend_from_slice(query.as_bytes());
        }
        head.extend_from_slice(b" HTTP/1.1\r\n");

        for (name, value) in self.headers.iter() {
            let replaced = reserved(name)
                || in_request(name)
                || (content_type.is_some() && name == "content-type");
            if !replaced {
                push(&mut head, name, value);
            }
        }
        if let Some(content_type) = content_type
            && !in_request("content-type")
        {
            push(&mut head, "content-type", content_type);
        }
        for (i, (name, value)) in request_headers.iter().enumerate() {
            // A later entry with the same name replaces this one.
            let replaced = request_headers[i + 1..]
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name));
            if !reserved(name) && !replaced {
                push(&mut head, name, value);
            }
        }
        if self.headers.get("accept-encoding").is_none() && !in_request("accept-encoding") {
            push(&mut head, "accept-encoding", "gzip, deflate");
        }

        let _ = write!(head, "host: {host}:{port}\r\nconnection: keep-alive\r\n");
        match content_length {
            Some(length) => {
                let _ = write!(head, "content-length: {length}\r\n");
            }
            None => head.extend_from_slice(b"transfer-encoding: chunked\r\n"),
        }
        head.extend_from_slice(b"\r\n");
        head
    }

    fn resolve_url(&self, value: &str) -> Result<Url, Error> {
        if let Ok(url) = Url::parse(value) {
            return Ok(url);
        }
        let base_url = self.base_url.as_ref().ok_or(Error::MissingBaseUrl)?;
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

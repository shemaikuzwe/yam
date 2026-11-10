use std::{format, sync::Arc, time::Duration};

use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use url::Url;
use yam_shared::Headers;

use crate::{
    error::Error,
    pool::Pools,
    request::{Body, Method, RequestOptions},
    response::Response,
    transport,
};

#[derive(Debug)]
pub struct HttpClient {
    base_url: Option<String>,
    timeout: Option<Duration>,
    tls_configuration: Arc<ClientConfig>,
    headers: Headers,
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

#[derive(Default)]
pub struct HttpClientConfig {
    /// Base URL used when a request is given a relative path.
    pub base_url: Option<String>,
    /// Maximum duration for the complete request and response exchange.
    pub timeout: Option<Duration>,
    /// TLS roots and client settings. Web PKI roots are used by default.
    pub tls_configuration: Option<ClientConfig>,
    /// Headers included with every request can be overridden per request.
    pub headers: Vec<(String, String)>,
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

        let mut request_target = url.path().to_string();
        if let Some(query) = url.query() {
            request_target.push('?');
            request_target.push_str(query);
        }

        let mut headers = self.headers.clone();
        let (body, chunks, content_type) = options
            .body
            .unwrap_or_else(|| Body::bytes(Vec::new()))
            .into_parts();
        if let Some(content_type) = content_type {
            headers.set("content-type", content_type);
        }
        for (name, value) in options.headers {
            headers.set(&name, value);
        }
        if headers.get("accept-encoding").is_none() {
            headers.set("accept-encoding", "gzip, deflate".into());
        }
        headers.set("host", format!("{host}:{port}"));
        headers.set("connection", "keep-alive".into());
        if chunks.is_some() {
            headers.set("transfer-encoding", "chunked".into());
        } else {
            headers.set("content-length", body.len().to_string());
        }

        let mut request_head = format!("{} {} HTTP/1.1\r\n", method.as_str(), request_target);
        for (name, value) in headers.iter() {
            if chunks.is_some() && name.eq_ignore_ascii_case("content-length") {
                continue;
            }
            request_head.push_str(&format!("{name}: {value}\r\n"));
        }
        request_head.push_str("\r\n");

        transport::send(
            &self.pools,
            &url,
            &self.tls_configuration,
            request_head.as_bytes(),
            body,
            chunks,
        )
        .await
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

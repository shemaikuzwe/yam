use std::collections::HashSet;

use yam_server::{HandlerFuture, IntoResponse, Method, Request, Response, StatusCode};

use crate::middleware::{Middleware, Next, Scope};

/// Creates Cors middleware
///
/// ```
/// use yam_router::{cors::Cors, router::Router};
///
/// let mut app = Router::new(Default::default());
/// app.middleware(Cors::new().allow_origin("http://localhost:5173").credentials(true));
/// ```
#[derive(Debug)]
pub struct Cors {
    /// Empty means any origin is allowed.
    origins: HashSet<String>,
    methods: Vec<Method>,
    headers: String,
    credentials: bool,
    max_age: Option<u32>,
}

impl Cors {
    /// Allows any origin, common methods and the `content-type`/`authorization` headers.
    pub fn new() -> Self {
        Cors {
            origins: HashSet::new(),
            methods: vec![
                Method::GET,
                Method::HEAD,
                Method::OPTIONS,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
            ],
            headers: "content-type, authorization".into(),
            credentials: false,
            max_age: None,
        }
    }

    /// Restricts allowed origins.
    pub fn allow_origin(mut self, origin: impl Into<String>) -> Self {
        self.origins.insert(origin.into());
        self
    }

    /// Sets `Access-Control-Allow-Methods`, e.g. `[Method::GET, Method::POST]`.
    pub fn allow_methods(mut self, methods: impl Into<Vec<Method>>) -> Self {
        self.methods = methods.into();
        self
    }

    /// Sets `Access-Control-Allow-Headers`, e.g. `"content-type, x-api-key"`.
    pub fn allow_headers(mut self, headers: impl Into<String>) -> Self {
        self.headers = headers.into();
        self
    }

    pub fn credentials(mut self, credentials: bool) -> Self {
        self.credentials = credentials;
        self
    }

    /// Sets how long (seconds) browsers may cache a preflight response.
    pub fn max_age(mut self, seconds: u32) -> Self {
        self.max_age = Some(seconds);
        self
    }
}

fn apply_cors(mut res: Response, origin: &str, credentials: bool) -> Response {
    res.headers
        .set("access-control-allow-origin", origin.to_string());
    res.headers.append("vary", "Origin".to_string());
    if credentials {
        res.headers
            .set("access-control-allow-credentials", "true".to_string());
    }
    res
}

impl Default for Cors {
    fn default() -> Self {
        Cors::new()
    }
}

impl Middleware for Cors {
    fn scope(&self) -> Scope {
        Scope::Global
    }

    fn call(&self, req: Request, next: Next) -> HandlerFuture {
        let origin = req
            .headers
            .get("origin")
            .filter(|origin| self.origins.is_empty() || self.origins.contains(*origin))
            .map(String::from);
        let Some(origin) = origin else {
            return next.run(req);
        };

        let is_preflight = req.method().eq_ignore_ascii_case("OPTIONS")
            && req.headers.get("access-control-request-method").is_some();
        if is_preflight {
            let methods = self
                .methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let mut res = apply_cors(
                Response::new().status(StatusCode::NoContent),
                &origin,
                self.credentials,
            )
            .set("access-control-allow-methods", &methods)
            .set("access-control-allow-headers", &self.headers);
            if let Some(max_age) = self.max_age {
                res = res.set("access-control-max-age", &max_age.to_string());
            }
            return Box::pin(async move { Ok(res) });
        }

        let credentials = self.credentials;
        Box::pin(async move {
            let res = next
                .run(req)
                .await
                .unwrap_or_else(|err| err.into_response());
            Ok(apply_cors(res, &origin, credentials))
        })
    }
}

#[cfg(test)]
mod tests {
    use yam_server::Handler;

    use super::*;
    use crate::router::{Router, RouterConfig};

    fn request(method: &str, path: &str, headers: &str) -> Request {
        let mut request = Request::new();
        request
            .parse(format!("{method} {path} HTTP/1.1\r\n{headers}\r\n").as_bytes())
            .expect("request should be valid");
        request
    }

    fn app(cors: Cors) -> Router {
        let mut router = Router::new(RouterConfig::default());
        router.middleware(cors);
        router.get("/users", |_| async { "users" });
        router.get("/users/{id}", |req: Request| async move {
            req.param_as::<u32>("id").map(|id| id.to_string())
        });
        router
    }

    #[tokio::test]
    async fn should_answer_preflight() {
        let router = app(Cors::new().credentials(true).max_age(600));
        let response = router
            .call(request(
                "OPTIONS",
                "/users",
                "Origin: http://a.com\r\nAccess-Control-Request-Method: POST\r\n",
            ))
            .await
            .unwrap();

        assert_eq!(response.status, StatusCode::NoContent);
        assert_eq!(
            response.headers.get("access-control-allow-origin"),
            Some("http://a.com")
        );
        assert_eq!(
            response.headers.get("access-control-allow-credentials"),
            Some("true")
        );
        assert_eq!(response.headers.get("access-control-max-age"), Some("600"));
        assert!(
            response
                .headers
                .get("access-control-allow-methods")
                .is_some()
        );
    }

    #[tokio::test]
    async fn should_add_headers_to_actual_request() {
        let router = app(Cors::new());
        let response = router
            .call(request("GET", "/users", "Origin: http://a.com\r\n"))
            .await
            .unwrap();

        assert_eq!(response.body, b"users");
        assert_eq!(
            response.headers.get("access-control-allow-origin"),
            Some("http://a.com")
        );
        assert_eq!(response.headers.get("vary"), Some("Origin"));
    }

    #[tokio::test]
    async fn should_skip_disallowed_origin() {
        let router = app(Cors::new().allow_origin("http://a.com"));
        let response = router
            .call(request("GET", "/users", "Origin: http://evil.com\r\n"))
            .await
            .unwrap();

        assert_eq!(response.headers.get("access-control-allow-origin"), None);
    }

    #[tokio::test]
    async fn should_add_headers_to_error_response() {
        let router = app(Cors::new());
        let response = router
            .call(request("GET", "/users/abc", "Origin: http://a.com\r\n"))
            .await
            .unwrap();

        assert_eq!(response.status, StatusCode::BadRequest);
        assert_eq!(
            response.headers.get("access-control-allow-origin"),
            Some("http://a.com")
        );
    }
}

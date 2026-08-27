use std::{collections::HashMap, format, future::Future, io, sync::Arc};

use matchit::Router as MatchRouter;
use tokio::net::TcpListener;
use yam_server::{
    Handler, HandlerFuture, Request, Response, Server, StatusCode, request::Method,
    response::IntoResponse,
};

use crate::middleware::{Middleware, Next};

pub struct Router {
    routes: HashMap<Method, MatchRouter<Arc<dyn Handler>>>,
    middlewares: Vec<Arc<dyn Middleware>>,
    trailing_slash: bool,
    route_prefix: String,
}

#[derive(Default)]
pub struct RouterConfig {
    pub strict_trailing_slash: bool,
    pub route_prefix: String,
}

macro_rules! route_verb {
    ($(#[$docs:meta])* $name:ident => $method:ident) => {
        $(#[$docs])*
        pub fn $name<R, F, Fut>(&mut self, path: &str, handler: F)
        where
            R: IntoResponse + Send + 'static,
            F: Fn(Request) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = R> + Send + 'static,
        {
            self.add_route(Method::$method, path, handler);
        }
    };
}

impl Router {
    pub fn new(config: RouterConfig) -> Router {
        Router {
            routes: HashMap::new(),
            middlewares: Vec::new(),
            trailing_slash: config.strict_trailing_slash,
            route_prefix: config.route_prefix,
        }
    }
    route_verb!(get => GET);
    route_verb!(post => POST);
    route_verb!(put => PUT);
    route_verb!(patch => PATCH);
    route_verb!(delete => DELETE);
    pub fn middleware<M>(&mut self, middleware: M)
    where
        M: Middleware,
    {
        self.middlewares.push(Arc::new(middleware));
    }
    fn add_route<H: Handler>(&mut self, method: Method, path: &str, handler: H) {
        let path = format!("{}{path}", self.route_prefix);
        let path = self.normalize_path(&path);
        self.routes
            .entry(method)
            .or_default()
            .insert(path, Arc::new(handler))
            .unwrap_or_else(|err| panic!("Failed to register route: {err}"))
    }
    pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
        Server::serve(listener, self).await
    }

    fn normalize_path<'a>(&self, path: &'a str) -> &'a str {
        if self.trailing_slash || path == "/" {
            path
        } else {
            path.trim_end_matches("/")
        }
    }
}

impl Handler for Router {
    fn call(&self, mut req: Request) -> HandlerFuture {
        let (Some(path), Some(method)) = (req.path(), req.method()) else {
            return Box::pin(async move {
                Ok(Response::new()
                    .status(StatusCode::StatusBadRequest)
                    .send("Bad request"))
            });
        };
        let path = self.normalize_path(path);
        let method = match Method::try_from(method) {
            Ok(method) => method,
            Err(_) => {
                return Box::pin(async move {
                    Ok(Response::new()
                        .status(StatusCode::StatusMethodNotAllowed)
                        .send("Method Not Allowed"))
                });
            }
        };
        let matched_route = self
            .routes
            .get(&method)
            .and_then(|router| router.at(path).ok());
        let (handler, params) = match matched_route {
            Some(matched) => {
                let handler = Arc::clone(matched.value);
                let params = matched
                    .params
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect::<HashMap<_, _>>();
                (handler, params)
            }
            None => {
                let path_exists = self.routes.values().any(|route| route.at(path).is_ok());
                let (status, message) = match path_exists {
                    true => (StatusCode::StatusMethodNotAllowed, "Method not allowed"),
                    false => (StatusCode::StatusNotFound, "Not Found"),
                };
                return Box::pin(async move { Ok(Response::new().status(status).send(message)) });
            }
        };
        req.set_params(params);
        Next::new(self.middlewares.clone(), handler).run(req)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;

    fn request(method: &str, path: &str) -> Request {
        let mut request = Request::new();
        request
            .parse(format!("{method} {path} HTTP/1.1\r\n\r\n").as_bytes())
            .expect("request should be valid");
        request
    }

    #[tokio::test]
    async fn should_dispatch_registered_get_route() {
        let mut router = Router::new(RouterConfig::default());
        router.get("/users", |_| async { "users" });

        let response = router
            .call(request("GET", "/users"))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::StatusOk);
        assert_eq!(response.body, b"users");
    }

    #[tokio::test]
    async fn should_dispatch_supported_http_methods() {
        let mut router = Router::new(RouterConfig::default());
        router.get("/get", |_| async { "GET" });
        router.post("/post", |_| async { "POST" });
        router.put("/put", |_| async { "PUT" });
        router.patch("/patch", |_| async { "PATCH" });
        router.delete("/delete", |_| async { "DELETE" });

        for (method, path) in [
            ("GET", "/get"),
            ("POST", "/post"),
            ("PUT", "/put"),
            ("PATCH", "/patch"),
            ("DELETE", "/delete"),
        ] {
            let response = router
                .call(request(method, path))
                .await
                .expect("handler should succeed");

            assert_eq!(response.status, StatusCode::StatusOk);
            assert_eq!(response.body, method.as_bytes());
        }
    }

    #[tokio::test]
    async fn should_extract_path_parameters() {
        let mut router = Router::new(RouterConfig::default());
        router.get("/users/{id}", |request| async move {
            request.param("id").unwrap_or("missing").to_string()
        });

        let response = router
            .call(request("GET", "/users/42"))
            .await
            .expect("handler should succeed");

        assert_eq!(response.body, b"42");
    }

    #[tokio::test]
    async fn should_return_not_found_for_unknown_path() {
        let router = Router::new(RouterConfig::default());

        let response = router
            .call(request("GET", "/missing"))
            .await
            .expect("router should return a response");

        assert_eq!(response.status, StatusCode::StatusNotFound);
    }

    #[tokio::test]
    async fn should_return_method_not_allowed_for_existing_path() {
        let mut router = Router::new(RouterConfig::default());
        router.get("/users", |_| async { "users" });

        let response = router
            .call(request("POST", "/users"))
            .await
            .expect("router should return a response");

        assert_eq!(response.status, StatusCode::StatusMethodNotAllowed);
    }

    #[tokio::test]
    async fn should_ignore_trailing_slashes_by_default() {
        let mut router = Router::new(RouterConfig::default());
        router.get("/users", |_| async { "users" });

        for path in ["/users", "/users/", "/users///"] {
            let response = router
                .call(request("GET", path))
                .await
                .expect("handler should succeed");

            assert_eq!(response.status, StatusCode::StatusOk);
        }
    }

    #[tokio::test]
    async fn should_respect_strict_trailing_slashes() {
        let mut router = Router::new(RouterConfig {
            strict_trailing_slash: true,
            ..RouterConfig::default()
        });
        router.get("/users", |_| async { "users" });

        let response = router
            .call(request("GET", "/users/"))
            .await
            .expect("router should return a response");

        assert_eq!(response.status, StatusCode::StatusNotFound);
    }

    #[tokio::test]
    async fn should_apply_route_prefix() {
        let mut router = Router::new(RouterConfig {
            route_prefix: "/api".to_string(),
            ..RouterConfig::default()
        });
        router.get("/users", |_| async { "users" });

        let response = router
            .call(request("GET", "/api/users"))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::StatusOk);
    }

    #[tokio::test]
    async fn should_run_middleware_in_registration_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut router = Router::new(RouterConfig::default());

        for name in ["first", "second"] {
            let events = Arc::clone(&events);
            router.middleware(move |request, next: Next| {
                let events = Arc::clone(&events);
                async move {
                    events.lock().unwrap().push(format!("{name}:before"));
                    let response = next.run(request).await;
                    events.lock().unwrap().push(format!("{name}:after"));
                    response
                }
            });
        }
        router.get("/users", |_| async { "users" });

        router
            .call(request("GET", "/users"))
            .await
            .expect("handler should succeed");

        assert_eq!(
            *events.lock().unwrap(),
            [
                "first:before",
                "second:before",
                "second:after",
                "first:after"
            ]
        );
    }

    #[tokio::test]
    async fn should_allow_middleware_to_short_circuit_handler() {
        let handler_called = Arc::new(AtomicBool::new(false));
        let mut router = Router::new(RouterConfig::default());
        router.middleware(|_request, _next: Next| async {
            Ok(Response::new()
                .status(StatusCode::StatusUnauthorized)
                .send("Unauthorized"))
        });

        let handler_called_by_route = Arc::clone(&handler_called);
        router.get("/users", move |_| {
            handler_called_by_route.store(true, Ordering::SeqCst);
            async { "users" }
        });

        let response = router
            .call(request("GET", "/users"))
            .await
            .expect("middleware should return a response");

        assert_eq!(response.status, StatusCode::StatusUnauthorized);
        assert!(!handler_called.load(Ordering::SeqCst));
    }
}

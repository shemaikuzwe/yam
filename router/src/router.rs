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
    /// When `true`, routes /users/ and /users are completely different.
    pub strict_trailing_slash: bool,
    /// A prefix added to every registered route, such as `/api/v1`.
    pub route_prefix: String,
}

macro_rules! route_verb {
    ($(#[$docs:meta])* $name:ident => $method:ident) => {
        $(#[$docs])*
        pub fn $name<R, F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
        where
            R: IntoResponse + Send + 'static,
            F: Fn(Request) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = R> + Send + 'static,
        {
            self.add_route(Method::$method, path, handler);
            self
        }
    };
}

impl Router {
    ///
    /// ```
    /// use yam_router::router::{Router, RouterConfig};
    ///
    /// let router = Router::new(RouterConfig {
    ///     route_prefix: "/api/v1".into(),
    ///     ..Default::default()
    /// });
    /// ```
    pub fn new(config: RouterConfig) -> Router {
        Router {
            routes: HashMap::new(),
            middlewares: Vec::new(),
            trailing_slash: config.strict_trailing_slash,
            route_prefix: config.route_prefix,
        }
    }
    route_verb!(
        #[doc = "```"]
        #[doc = "use yam_router::router::Router;"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = ""]
        #[doc = "app.get(\"/\", async |_request| {"]
        #[doc = "    \"Hello world\""]
        #[doc = "});"]
        #[doc = "```"]
        get => GET
    );
    route_verb!(
        #[doc = "```"]
        #[doc = "use serde::Deserialize;"]
        #[doc = "use serde_json::json;"]
        #[doc = "use yam_router::router::Router;"]
        #[doc = "use yam_server::{HttpError, Request, Response, StatusCode};"]
        #[doc = ""]
        #[doc = "#[derive(Deserialize)]"]
        #[doc = "struct CreateUser { email: String }"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = "app.post(\"/users\", async |request: Request| -> Result<Response, HttpError> {"]
        #[doc = "    let input: CreateUser = request.json()?;"]
        #[doc = "    Ok(Response::new()"]
        #[doc = "        .status(StatusCode::StatusCreated)"]
        #[doc = "        .json(&json!({ \"id\": 42, \"email\": input.email })))"]
        #[doc = "});"]
        #[doc = "```"]
        post => POST
    );
    route_verb!(
        #[doc = "```"]
        #[doc = "use serde::{Deserialize, Serialize};"]
        #[doc = "use yam_router::router::Router;"]
        #[doc = "use yam_server::{HttpError, Json, Request};"]
        #[doc = ""]
        #[doc = "#[derive(Deserialize)]"]
        #[doc = "struct ReplaceUser { email: String, name: String }"]
        #[doc = ""]
        #[doc = "#[derive(Serialize)]"]
        #[doc = "struct User { id: u64, email: String, name: String }"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = "app.put(\"/users/{id}\", async |request: Request| -> Result<Json<User>, HttpError> {"]
        #[doc = "    let id: u64 = request.param_as(\"id\")?;"]
        #[doc = "    let input: ReplaceUser = request.json()?;"]
        #[doc = "    Ok(Json(User { id, email: input.email, name: input.name }))"]
        #[doc = "});"]
        #[doc = "```"]
        put => PUT
    );
    route_verb!(
        #[doc = "```"]
        #[doc = "use serde::Deserialize;"]
        #[doc = "use serde_json::json;"]
        #[doc = "use yam_router::router::Router;"]
        #[doc = "use yam_server::{HttpError, Request};"]
        #[doc = ""]
        #[doc = "#[derive(Deserialize)]"]
        #[doc = "struct UpdateUser { name: Option<String> }"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = "app.patch(\"/users/{id}\", async |request: Request| -> Result<serde_json::Value, HttpError> {"]
        #[doc = "    let id: u64 = request.param_as(\"id\")?;"]
        #[doc = "    let input: UpdateUser = request.json()?;"]
        #[doc = "    Ok(json!({"]
        #[doc = "        \"id\": id,"]
        #[doc = "        \"name\": input.name,"]
        #[doc = "    }))"]
        #[doc = "});"]
        #[doc = "```"]
        patch => PATCH
    );
    route_verb!(
        #[doc = "```"]
        #[doc = "use yam_router::router::Router;"]
        #[doc = "use yam_server::{Request, Response, StatusCode};"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = "app.delete(\"/users/{id}\", async |request: Request| {"]
        #[doc = "    let id: u64 = request.param_as(\"id\")?;"]
        #[doc = "    // Delete the user identified by `id` from storage."]
        #[doc = "    let _ = id;"]
        #[doc = "    Ok(Response::new().status(StatusCode::StatusNoContent))"]
        #[doc = "});"]
        #[doc = "```"]
        delete => DELETE
    );
    /// Appends middleware that will wrap every matched route.
    ///
    /// Call [`Next::run`] to continue to the next middleware or route handler.
    ///
    /// ```
    /// use yam_router::{Next, router::Router};
    /// use yam_server::Request;
    ///
    /// let mut app = Router::new(Default::default());
    /// app.middleware(async |request: Request, next: Next| {
    ///     println!("before handler");
    ///     let response = next.run(request).await?;
    ///     println!("after handler");
    ///     Ok(response)
    /// });
    /// ```
    pub fn middleware<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware,
    {
        self.middlewares.push(Arc::new(middleware));
        self
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
    /// Serves router on an already-bound TCP listener.
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    /// use yam_router::router::Router;
    ///
    /// # async fn run() -> std::io::Result<()> {
    /// let mut app = Router::new(Default::default());
    /// app.get("/", async |_| "Hello world");
    ///
    /// let listener = TcpListener::bind("localhost:3000").await?;
    /// app.serve(listener).await
    /// # }
    /// ```
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
        let path = self.normalize_path(req.path());
        let method = match Method::try_from(req.method()) {
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

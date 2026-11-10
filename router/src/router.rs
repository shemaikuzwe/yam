use std::{format, future::Future, io, sync::Arc};

use tokio::net::TcpListener;
use yam_server::{
    Handler, HandlerFuture, Request, Response, Server, StatusCode, request::Method,
    response::IntoResponse,
};

use crate::{
    middleware::{Middleware, Middlewares, Next},
    routes::{Lookup, Routes},
};

pub struct Router {
    routes: Routes,
    middlewares: Middlewares,
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
            routes: Routes::new(config.strict_trailing_slash),
            middlewares: Arc::default(),
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
        #[doc = "        .status(StatusCode::Created)"]
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
        #[doc = "use yam_server::{HttpError, Request, Response, StatusCode};"]
        #[doc = ""]
        #[doc = "let mut app = Router::new(Default::default());"]
        #[doc = "app.delete(\"/users/{id}\", async |request: Request| -> Result<Response, HttpError> {"]
        #[doc = "    let id: u64 = request.param_as(\"id\")?;"]
        #[doc = "    // Delete the user identified by `id` from storage."]
        #[doc = "    let _ = id;"]
        #[doc = "    Ok(Response::new().status(StatusCode::NoContent))"]
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
        let scope = middleware.scope();
        Arc::make_mut(&mut self.middlewares).push((Arc::new(middleware), scope));
        self
    }
    fn add_route<H: Handler>(&mut self, method: Method, path: &str, handler: H) {
        let path = format!("{}{path}", self.route_prefix);
        self.routes.insert(method, &path, Arc::new(handler));
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
}

impl Handler for Router {
    fn call(&self, mut req: Request) -> HandlerFuture {
        let (handler, matched) = match self.routes.find(req.method(), req.path()) {
            Lookup::Found { handler, params } => {
                req.set_params(params);
                (handler, true)
            }
            Lookup::WrongMethod { allow } if req.method().eq_ignore_ascii_case("OPTIONS") => {
                (fallback(StatusCode::NoContent, Some(allow), ""), true)
            }
            Lookup::WrongMethod { allow } => (
                fallback(
                    StatusCode::MethodNotAllowed,
                    Some(allow),
                    "Method not allowed",
                ),
                false,
            ),
            Lookup::NotFound => (fallback(StatusCode::NotFound, None, "Not Found"), false),
        };
        Next::new(Arc::clone(&self.middlewares), handler, matched).run(req)
    }
}

fn fallback(status: StatusCode, allow: Option<String>, message: &'static str) -> Arc<dyn Handler> {
    Arc::new(move |_req: Request| {
        let allow = allow.clone();
        async move {
            let response = Response::new().status(status);
            let response = match &allow {
                Some(allow) => response.set("allow", allow),
                None => response,
            };
            response.send(message)
        }
    })
}

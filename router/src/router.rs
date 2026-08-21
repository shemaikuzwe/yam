use std::{collections::HashMap, future::Future, io, sync::Arc};

use matchit::Router as MatchRouter;
use tokio::net::TcpListener;
use yam_server::{
    Handler, HandlerFuture, Request, Response, Server, StatusCode, request::Method,
    response::IntoResponse,
};

pub struct Router {
    routes: HashMap<Method, MatchRouter<Arc<dyn Handler>>>,
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
    pub fn new() -> Router {
        Router {
            routes: HashMap::new(),
        }
    }
    route_verb!(get => GET);
    route_verb!(post => POST);
    route_verb!(put => PUT);
    route_verb!(patch => PATCH);
    route_verb!(delete => DELETE);

    fn add_route<H: Handler>(&mut self, method: Method, path: &str, handler: H) {
        self.routes
            .entry(method)
            .or_default()
            .insert(path, Arc::new(handler))
            .unwrap_or_else(|err| panic!("Failed to register route: {err}"))
    }
    pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
        Server::serve(listener, self).await
    }
}

impl Handler for Router {
    fn call(&self, mut req: Request) -> HandlerFuture {
        let Some(request_line) = &req.request_line else {
            return Box::pin(async move {
                Ok(Response::new()
                    .status(StatusCode::StatusBadRequest)
                    .send("Bad request"))
            });
        };
        let method = match Method::try_from(request_line.method.as_str()) {
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
            .and_then(|router| router.at(&request_line.request_target).ok());
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
                let path_exists = self
                    .routes
                    .values()
                    .any(|route| route.at(&request_line.request_target).is_ok());
                let (status, message) = match path_exists {
                    true => (StatusCode::StatusMethodNotAllowed, "Method not allowed"),
                    false => (StatusCode::StatusNotFound, "Not Found"),
                };
                return Box::pin(async move { Ok(Response::new().status(status).send(message)) });
            }
        };
        req.set_params(params);
        handler.call(req)
    }
}

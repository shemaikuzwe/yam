use std::{collections::HashMap, future::Future, io, sync::Arc};

use tokio::net::TcpListener;
use yam_server::{
    request::Method, response::HttpError, response::IntoResponse, Handler, HandlerFuture, Request,
    Response, Server, StatusCode,
};

pub struct Router {
    routes: HashMap<(Method, String), Arc<dyn Handler>>,
}

macro_rules! route_verb {
    ($(#[$docs:meta])* $name:ident => $method:ident) => {
        $(#[$docs])*
        pub fn $name<R, F, Fut>(&mut self, path: &str, handler: F)
        where
            R: IntoResponse + Send + 'static,
            F: Fn(Request) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<R, HttpError>> + Send + 'static,
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
            .insert((method, path.to_string()), Arc::new(handler));
    }
    pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
        Server::serve(listener, self).await
    }
}

impl Handler for Router {
    fn call(&self, req: Request) -> HandlerFuture {
        let Some(request_line) = &req.request_line else {
            return Box::pin(async move {
                Ok(Response::new()
                    .status(StatusCode::StatusBadRequest)
                    .send("Bad request"))
            });
        };
        let method = Method::from(request_line.method.as_str());
        match self
            .routes
            .get(&(method, request_line.request_target.clone()))
        {
            Some(handler) => handler.call(req),
            None => Box::pin(async move {
                Ok(Response::new()
                    .status(StatusCode::StatusNotFound)
                    .send("Not Found"))
            }),
        }
    }
}
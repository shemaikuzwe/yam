use std::{collections::HashMap, future::Future, io, sync::Arc};

use tokio::net::{TcpListener, TcpStream};
use yam_server::{Handler, HandlerFuture, Request, Response, Server, StatusCode, request::Method};

pub struct Router {
    routes: HashMap<(Method, String), Arc<dyn Handler>>,
}

impl Router {
    pub fn new() -> Router {
        Router {
            routes: HashMap::new(),
        }
    }
    pub fn get<T>(
        &mut self,
        path: &str,
        handler: impl Fn(Request, Response<TcpStream>) -> T + Send + Sync + 'static,
    ) where
        T: Future<Output = io::Result<()>> + Send + 'static,
    {
        self.add_route(Method::GET, path, handler);
    }
    pub fn post<T>(
        &mut self,
        path: &str,
        handler: impl Fn(Request, Response<TcpStream>) -> T + Send + Sync + 'static,
    ) where
        T: Future<Output = io::Result<()>> + Send + 'static,
    {
        self.add_route(Method::POST, path, handler);
    }
    pub fn put<T>(
        &mut self,
        path: &str,
        handler: impl Fn(Request, Response<TcpStream>) -> T + Send + Sync + 'static,
    ) where
        T: Future<Output = io::Result<()>> + Send + 'static,
    {
        self.add_route(Method::PUT, path, handler);
    }
    pub fn patch<T>(
        &mut self,
        path: &str,
        handler: impl Fn(Request, Response<TcpStream>) -> T + Send + Sync + 'static,
    ) where
        T: Future<Output = io::Result<()>> + Send + 'static,
    {
        self.add_route(Method::PATCH, path, handler);
    }
    pub fn delete<T>(
        &mut self,
        path: &str,
        handler: impl Fn(Request, Response<TcpStream>) -> T + Send + Sync + 'static,
    ) where
        T: Future<Output = io::Result<()>> + Send + 'static,
    {
        self.add_route(Method::DELETE, path, handler);
    }

    fn add_route<H: Handler>(&mut self, method: Method, path: &str, handler: H) {
        self.routes
            .insert((method, path.to_string()), Arc::new(handler));
    }
    pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
        Server::serve(listener, self).await
    }
}

impl Handler for Router {
    fn call(&self, req: Request, res: Response<TcpStream>) -> HandlerFuture {
        let Some(request_line) = &req.request_line else {
            return Box::pin(async move {
                res.status(StatusCode::StatusBadRequest)
                    .send("Bad request")
                    .await
            });
        };
        let method = Method::from(request_line.method.as_str());
        match self
            .routes
            .get(&(method, request_line.request_target.clone()))
        {
            Some(handler) => handler.call(req, res),
            None => Box::pin(async move {
                res.status(StatusCode::StatusNotFound)
                    .send("Not Found")
                    .await
            }),
        }
    }
}

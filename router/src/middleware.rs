use std::sync::Arc;

use yam_server::{Handler, HandlerFuture, HttpError, Request, Response};

pub trait Middleware: Send + Sync + 'static {
    fn call(&self, req: Request, next: Next) -> HandlerFuture;
}

/// The remaining middleware chain and final route handler.
pub struct Next {
    middlewares: Arc<Vec<Arc<dyn Middleware>>>,
    handler: Arc<dyn Handler>,
    index: usize,
}

impl Next {
    pub fn new(middlewares: Vec<Arc<dyn Middleware>>, handler: Arc<dyn Handler>) -> Self {
        Self {
            middlewares: Arc::new(middlewares),
            handler,
            index: 0,
        }
    }
    /// Continues processing the request through the remaining middleware.
    pub fn run(self, req: Request) -> HandlerFuture {
        let Some(middleware) = self.middlewares.get(self.index) else {
            return self.handler.call(req);
        };

        let next = Next {
            middlewares: Arc::clone(&self.middlewares),
            handler: Arc::clone(&self.handler),
            index: self.index + 1,
        };
        middleware.call(req, next)
    }
}

impl<F, Fut> Middleware for F
where
    F: Fn(Request, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
{
    fn call(&self, req: Request, next: Next) -> HandlerFuture {
        Box::pin(self(req, next))
    }
}

use std::sync::Arc;

use yam_server::{Handler, HandlerFuture, HttpError, Request, Response};

pub trait Middleware: Send + Sync + 'static {
    fn call(&self, req: Request, next: Next) -> HandlerFuture;

    fn scope(&self) -> Scope {
        Scope::Router
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Router,
    Path(&'static str),
}

impl Scope {
    fn applies(self, matched: bool, path: &str) -> bool {
        match self {
            Scope::Global => true,
            Scope::Router => matched,
            Scope::Path(prefix) => {
                matched
                    && path
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            }
        }
    }
}

pub(crate) type Middlewares = Arc<Vec<(Arc<dyn Middleware>, Scope)>>;

pub struct Next {
    middlewares: Middlewares,
    handler: Arc<dyn Handler>,
    index: usize,
    matched: bool,
}

impl Next {
    pub(crate) fn new(middlewares: Middlewares, handler: Arc<dyn Handler>, matched: bool) -> Self {
        Self {
            middlewares,
            handler,
            index: 0,
            matched,
        }
    }
    pub fn run(self, req: Request) -> HandlerFuture {
        let skipped = self.middlewares[self.index..]
            .iter()
            .position(|(_, scope)| scope.applies(self.matched, req.path()));
        let Some(skipped) = skipped else {
            return self.handler.call(req);
        };
        let index = self.index + skipped;

        let next = Next {
            middlewares: Arc::clone(&self.middlewares),
            handler: Arc::clone(&self.handler),
            index: index + 1,
            matched: self.matched,
        };
        self.middlewares[index].0.call(req, next)
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

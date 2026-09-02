use std::{collections::HashSet, time::Instant};

use tracing::Level;
use yam_server::{HandlerFuture, Request};

use crate::middleware::{Middleware, Next, Scope};

/// Emits structured `tracing` logs per request.
///
/// ```
/// use yam_router::{logger::Logger, router::Router};
///
/// let mut app = Router::new(Default::default());
/// app.middleware(Logger::new().exclude("/health"));
/// ```
///
#[derive(Debug)]
pub struct Logger {
    level: Level,
    exclude: HashSet<String>,
}

macro_rules! log_request {
    ($level:expr, $($arg:tt)*) => {
        match $level {
            Level::ERROR => tracing::error!($($arg)*),
            Level::WARN => tracing::warn!($($arg)*),
            Level::INFO => tracing::info!($($arg)*),
            Level::DEBUG => tracing::debug!($($arg)*),
            Level::TRACE => tracing::trace!($($arg)*),
        }
    };
}

impl Logger {
    /// Creates a logger that emits at [`Level::INFO`].
    pub fn new() -> Self {
        Logger {
            level: Level::INFO,
            exclude: HashSet::new(),
        }
    }

    /// Sets the log level to `level`
    /// By default log level is `Level::Info`
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Skips logging for an exact request path, such as a health check.
    pub fn exclude(mut self, path: impl Into<String>) -> Self {
        self.exclude.insert(path.into());
        self
    }
}

impl Default for Logger {
    fn default() -> Self {
        Logger::new()
    }
}

impl Middleware for Logger {
    fn scope(&self) -> Scope {
        Scope::Global
    }

    fn call(&self, req: Request, next: Next) -> HandlerFuture {
        if self.exclude.contains(req.path()) {
            return next.run(req);
        }

        let level = self.level;
        let method = req.method().to_string();
        let path = req.path().to_string();
        let start = Instant::now();

        Box::pin(async move {
            let result = next.run(req).await;
            let handler_ms = start.elapsed().as_secs_f64() * 1000.0;
            let handler_ms = format!("{:.3}", handler_ms);
            match &result {
                Ok(response) => log_request!(
                    level,
                    method = %method,
                    path = %path,
                    status = response.status as u16,
                    size = response.body.len(),
                    handler_ms= handler_ms,
                    "request completed"
                ),
                Err(err) => tracing::error!(
                    method = %method,
                    path = %path,
                    status = err.status() as u16,
                    handler_ms = handler_ms,
                    error = %err,
                    "request failed"
                ),
            }

            result
        })
    }
}

#[cfg(test)]
mod tests {
    use yam_server::{Handler, StatusCode};

    use super::*;
    use crate::router::{Router, RouterConfig};

    fn request(method: &str, path: &str) -> Request {
        let mut request = Request::new();
        request
            .parse(format!("{method} {path} HTTP/1.1\r\n\r\n").as_bytes())
            .expect("request should be valid");
        request
    }

    #[tokio::test]
    async fn should_pass_response_through_unchanged() {
        let mut router = Router::new(RouterConfig::default());
        router.middleware(Logger::new());
        router.get("/users", |_| async { "users" });

        let response = router
            .call(request("GET", "/users"))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::StatusOk);
        assert_eq!(response.body, b"users");
    }

    #[tokio::test]
    async fn should_pass_excluded_path_through_unchanged() {
        let mut router = Router::new(RouterConfig::default());
        router.middleware(Logger::new().exclude("/health"));
        router.get("/health", |_| async { "ok" });

        let response = router
            .call(request("GET", "/health"))
            .await
            .expect("handler should succeed");

        assert_eq!(response.status, StatusCode::StatusOk);
        assert_eq!(response.body, b"ok");
    }
}

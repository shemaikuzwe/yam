//! yam Router.
//!
//! ```no_run
//! use tokio::net::TcpListener;
//! use yam_router::router::{Router, RouterConfig};
//! use yam_server::{HttpError, Request, Response};
//!
//! # async fn run() -> std::io::Result<()> {
//! let mut app = Router::new(RouterConfig {
//!     route_prefix: "/api".into(),
//!     ..Default::default()
//! });
//! app.get("/users/{id}", async |request: Request| -> Result<Response, HttpError> {
//!     let id: u32 = request.param_as("id")?;
//!     Ok(Response::new().send(format!("user {id}")))
//! });
//!
//! let listener = TcpListener::bind("localhost:3000").await?;
//! app.serve(listener).await
//! # }
//! ```

pub mod logger;
pub mod middleware;
pub mod router;

pub use logger::Logger;
pub use middleware::{Middleware, Next};

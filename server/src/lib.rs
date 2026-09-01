//! HTTP/1.1 request parsing, response building, and TCP serving primitives.
//!
//! [`Server`] accepts connections, parses each request, passes it to a handler,
//! and writes the returned response.
//!
//! ```no_run
//! use tokio::net::TcpListener;
//! use yam_server::{Request, Response, Server};
//!
//! # async fn run() -> std::io::Result<()> {
//! let listener = TcpListener::bind("localhost:3000").await?;
//! Server::serve(listener, |_request: Request| async {
//!     Response::new().send("Hello world")
//! }).await
//! # }
//! ```

pub mod cookie;
pub mod headers;
pub mod request;
pub mod response;
pub mod server;

pub use headers::Headers;

pub use cookie::{Cookie, SameSite};
pub use request::{Error, Request, RequestLine};
pub use response::{HttpError, IntoResponse, Json, Response, ResponseWriter, StatusCode};
pub use server::{Handler, HandlerFuture, Server};

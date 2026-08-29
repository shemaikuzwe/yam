//! Yam http server, router, and client.

/// ```no_run
/// use tokio::net::TcpListener;
/// use yam::{router::router::Router, server::Response};
///
/// # async fn run() -> std::io::Result<()> {
/// let mut app = Router::new(Default::default());
/// app.get("/", async |_| Response::new().send("Hello from Yam"));
///
/// let listener = TcpListener::bind("localhost:3000").await?;
/// app.serve(listener).await
/// # }
/// ```
#[cfg(feature = "router")]
pub use yam_router as router;

/// ```no_run
/// use serde::Deserialize;
/// use yam::client::client::{Error, HttpClient, HttpClientConfig, RequestOptions};
///
/// #[derive(Deserialize)]
/// struct User { id: u64 }
///
/// # async fn run() -> Result<(), Error> {
/// let client = HttpClient::new(HttpClientConfig {
///     base_url: Some("https://example.com/api".into()),
///     ..Default::default()
/// });
/// let response = client.get("/users/1", RequestOptions::default()).await?;
/// let user: User = response.json()?;
/// # let _ = user.id;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "client")]
pub use yam_client as client;

#[cfg(feature = "server")]
pub use yam_server as server;

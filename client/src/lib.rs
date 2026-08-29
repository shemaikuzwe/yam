//! Yam client
//!
//! ```no_run
//! use serde::Deserialize;
//! use yam_client::client::{Error, HttpClient, HttpClientConfig, RequestOptions};
//!
//! #[derive(Deserialize)]
//! struct User { id: u64 }
//!
//! # async fn run() -> Result<(), Error> {
//! let client = HttpClient::new(HttpClientConfig {
//!     base_url: Some("https://example.com/api".into()),
//!     ..Default::default()
//! });
//! let response = client.get("/users/1", RequestOptions::default()).await?;
//! let user: User = response.json()?;
//! # let _ = user.id;
//! # Ok(())
//! # }
//! ```

pub mod client;

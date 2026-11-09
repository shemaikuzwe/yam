//!
//!
//! ```
//! use yam_server::Headers;
//!
//! let mut headers = Headers::new();
//! headers.set("content-type", "text/plain".into());
//!
//! assert_eq!(headers.get("Content-Type"), Some("text/plain"));
//! ```

pub use yam_shared::Headers;

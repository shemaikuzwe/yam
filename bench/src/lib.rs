//! Shared bits so every server returns byte-identical responses.
//!
//! Each binary in `src/bin` serves the same routes:
//! - `GET /`           -> `Hello, World!`
//! - `GET /json`       -> `{"message":"Hello, World!"}`
//! - `GET /users/{id}` -> the `id` echoed back

use serde::Serialize;

pub const ADDR: &str = "127.0.0.1:3000";
pub const HELLO: &str = "Hello, World!";

#[derive(Serialize)]
pub struct Message {
    pub message: &'static str,
}

pub const MESSAGE: Message = Message { message: HELLO };

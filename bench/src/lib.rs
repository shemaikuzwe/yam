//! Shared bits so every server returns byte-identical responses.
//!
//! Each binary in `src/bin` serves the same routes:
//! - `GET /`           -> `Hello, World!`
//! - `GET /json`       -> `{"message":"Hello, World!"}`
//! - `GET /users/{id}` -> the `id` echoed back

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const ADDR: &str = "127.0.0.1:3000";
pub const HELLO: &str = "Hello, World!";

#[derive(Serialize, Deserialize)]
pub struct Message<'a> {
    pub message: &'a str,
}

pub const MESSAGE: Message<'static> = Message { message: HELLO };

/// Validates a response body the same way for every client bin.
pub fn check(route: &str, body: &[u8]) {
    if route == "/json" {
        let msg: Message = serde_json::from_slice(body).expect("bad json");
        assert_eq!(msg.message, HELLO);
    } else {
        assert_eq!(body, HELLO.as_bytes());
    }
}

/// Client load loop: `CONNECTIONS` workers call `send(route)` back to back for
/// `DURATION_SECS` (after a 2s warm-up), then prints `{"rps","p50","p99"}`.
/// The route comes from the first CLI argument.
pub async fn load<F, Fut>(send: F)
where
    F: Fn(&'static str) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let route: &'static str = std::env::args()
        .nth(1)
        .expect("usage: <bin> <route>")
        .leak();
    let env = |key: &str, default: u64| {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let conns = env("CONNECTIONS", 16);
    let secs = env("DURATION_SECS", 10);

    run(&send, route, conns, Duration::from_secs(2)).await;
    let mut lat = run(&send, route, conns, Duration::from_secs(secs)).await;
    lat.sort();
    let pct = |p: f64| lat[(lat.len() as f64 * p) as usize].as_secs_f64() * 1000.0;
    println!(
        "{}",
        serde_json::json!({ "rps": lat.len() as f64 / secs as f64, "p50": pct(0.5), "p99": pct(0.99) })
    );
}

async fn run<F, Fut>(send: &F, route: &'static str, conns: u64, dur: Duration) -> Vec<Duration>
where
    F: Fn(&'static str) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let end = Instant::now() + dur;
    let tasks: Vec<_> = (0..conns)
        .map(|_| {
            let send = send.clone();
            tokio::spawn(async move {
                let mut lat = Vec::new();
                while Instant::now() < end {
                    let t = Instant::now();
                    send(route).await;
                    lat.push(t.elapsed());
                }
                lat
            })
        })
        .collect();
    let mut all = Vec::new();
    for task in tasks {
        all.extend(task.await.expect("worker panicked"));
    }
    all
}

use axum::{Json, Router, extract::Path, routing::get};
use bench::{ADDR, HELLO, MESSAGE};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(async || HELLO))
        .route("/json", get(async || Json(MESSAGE)))
        .route("/users/{id}", get(async |Path(id): Path<String>| id));

    axum::serve(TcpListener::bind(ADDR).await?, app).await
}

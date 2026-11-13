use bench::{ADDR, HELLO, MESSAGE};
use tokio::net::TcpListener;
use yam_http::{
    router::Router,
    server::{Json, Request},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = Router::new(Default::default());
    app.get("/", async |_| HELLO)
        .get("/json", async |_| Json(MESSAGE))
        .get("/users/{id}", async |req: Request| {
            req.param("id").unwrap_or_default().to_string()
        });

    app.serve(TcpListener::bind(ADDR).await?).await
}

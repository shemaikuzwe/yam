use serde_json::json;
use yam_http::router::{
    Logger,
    router::{Router, RouterConfig},
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let mut app = Router::new(RouterConfig {
        route_prefix: "/api".into(),
        ..Default::default()
    });
    app.get("/users", async |_| {
        json!([
            { "id": 1, "name": "Alice", "email": "alice@example.com" },
            { "id": 2, "name": "Bob", "email": "bob@example.com" },
            { "id": 3, "name": "Carol", "email": "carol@example.com" },
        ])
    })
    .static_files("/", concat!(env!("CARGO_MANIFEST_DIR"), "/public"))
    .middleware(Logger::default());
    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .unwrap();

    println!("Open http://localhost:3000/index.html");
    app.serve(listener).await.unwrap()
}

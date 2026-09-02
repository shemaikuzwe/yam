# Yam
Yam is a minimal http client,router and server:

> **⚠️ Warning:** This project is meant for learning purposes only. It is **not** meant for production use.

## Installation

```sh
cargo add yam-http
```

This enables the default features, `server` and `router`. The client is opt-in:

```sh
cargo add yam --features client
```

| Feature | Default | Provides |
| --- | --- | --- |
| `server` | yes | `yam::server` |
| `router` | yes | `yam::router` (enables `server`) |
| `client` | no | `yam::client` |

## Router
Heavily inspired by [express](https://expressjs.com/)

```rust
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use yam_router::router::{Router, RouterConfig};
use yam_server::{Cookie, Request, Response, SameSite, StatusCode};

#[derive(Deserialize)]
struct LoginPayload {
    email: String,
    password: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = Router::new(RouterConfig {
        route_prefix: "/api/v1".into(),
        ..Default::default()
    });

    app.get("/users/{id}", async |request: Request| {
        let id: u32 = request.param_as("id")?;
        Ok(json!({ "id": id }))
    });

    app.post("/users", async |request: Request| {
        let login: LoginPayload = request.json()?;
        let user = json!({ "email": login.email });
        let cookie = Cookie::new("auth.token", "user-id")
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax);

        Response::new()
            .status(StatusCode::StatusCreated)
            .cookie(cookie)
            .json(&user)
    });

    let listener = TcpListener::bind("localhost:3000").await?;
    app.serve(listener).await
}
```

## Client

Heavily inspired by [axios](https://axios-http.com/)

```rust
use serde::Deserialize;
use serde_json::json;
use yam_client::client::{Body, Error, HttpClient, HttpClientConfig, RequestOptions};

#[derive(Debug, Deserialize)]
struct User {
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some("http://localhost:3000/api/v1".into()),
        ..Default::default()
    });

    let response = client
        .post(
            "/users",
            RequestOptions {
                body: Some(Body::json(&json!({
                    "email": "user@example.com",
                    "password": "secret"
                }))?),
                ..Default::default()
            },
        )
        .await?;

    let user: User = response.json()?;
    println!("{user:#?}");
    Ok(())
}

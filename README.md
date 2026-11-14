# Yam
Yam is a minimal http client,router and server:

> **⚠️ Warning:** This project is meant for learning purposes only. It is **not** meant for production use.

## Installation

```sh
cargo add yam-http
```

This enables the default features, `server` and `router`. The client is opt-in:

```sh
cargo add yam-http --features client
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
            .status(StatusCode::Created)
            .cookie(cookie)
            .json(&user)
    });

    let listener = TcpListener::bind("localhost:3000").await?;
    app.serve(listener, Default::default()).await
}
```

## Client

Heavily inspired by [axios](https://axios-http.com/)

```rust
use serde::Deserialize;
use serde_json::json;
use yam_client::{Body, Error, HttpClient, HttpClientConfig, RequestOptions};

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
```

## Benchmarks

Measured in CI against popular Rust frameworks and clients. Higher req/s and lower latency are better. Gaps under ±10% are CI noise and shown as **≈ same**.

### Server

Averaged over 3 routes.

| Framework | req/s | vs yam | p50 (ms) | p99 (ms) |
| --- | --: | --: | --: | --: |
| **yam** | 155,642 | — | 0.40 | 0.82 |
| axum | 155,202 | ≈ same | 0.40 | 0.86 |
| hyper | 155,111 | ≈ same | 0.40 | 0.85 |
| actix | 155,085 | ≈ same | 0.37 | 0.77 |

<details>
<summary>Per route</summary>

| Route | Framework | req/s | vs yam | p50 (ms) | p99 (ms) |
| --- | --- | --: | --: | --: | --: |
| `/` | hyper | 157,334 | ≈ same | 0.40 | 0.80 |
| `/` | axum | 157,201 | ≈ same | 0.39 | 0.87 |
| `/` | **yam** | 155,579 | — | 0.40 | 0.81 |
| `/` | actix | 155,261 | ≈ same | 0.37 | 0.81 |
| `/json` | hyper | 156,390 | ≈ same | 0.40 | 0.82 |
| `/json` | **yam** | 154,533 | — | 0.40 | 0.85 |
| `/json` | axum | 154,362 | ≈ same | 0.40 | 0.87 |
| `/json` | actix | 154,309 | ≈ same | 0.36 | 0.80 |
| `/users/42` | **yam** | 156,813 | — | 0.40 | 0.80 |
| `/users/42` | actix | 155,685 | ≈ same | 0.37 | 0.70 |
| `/users/42` | axum | 154,043 | ≈ same | 0.40 | 0.83 |
| `/users/42` | hyper | 151,608 | ≈ same | 0.40 | 0.91 |

</details>


### Client

Averaged over `/` and `/json`.

| Client | req/s | vs yam-client | p50 (ms) | p99 (ms) |
| --- | --: | --: | --: | --: |
| hyper | 110,590 | ≈ same | 0.14 | 0.29 |
| **yam-client** | 109,746 | — | 0.14 | 0.24 |
| reqwest | 87,098 | 1.3× slower | 0.17 | 0.43 |

<details>
<summary>Setup</summary>

</details>

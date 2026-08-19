use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use yam_router::router::Router;
use yam_server::{HttpError, IntoResponse, Request, Response};

#[derive(Deserialize)]
struct LoginPaylod {
    email: String,
    password: String,
}

#[tokio::main]
async fn main() {
    let mut app = Router::new();
    app.get("/", |_req| async {
        let response = Response::new();
        Ok(response.send("hello world"))
    });
    app.get("/users", get_users);
    app.post("/users", async |req| {
        let data: LoginPaylod = req.json()?;
        let name = data.email.split('@').next();
        let user = json!({
            "id": "2234",
            "email": data.email,
            "name": name
        });
        Ok(user)
    });
    let listener = TcpListener::bind("localhost:3000").await.unwrap();
    app.serve(listener).await.unwrap();
}

async fn get_users(_req: Request) -> Result<impl IntoResponse, HttpError> {
    Ok("hello")
}

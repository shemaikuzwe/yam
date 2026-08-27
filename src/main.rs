use std::{println, time::Duration};

use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use yam_router::{
    Next,
    router::{Router, RouterConfig},
};
use yam_server::{Cookie, Request, Response};

#[derive(Deserialize)]
struct LoginPaylod {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct Pagination {
    page: Option<u64>,
    per_page: Option<u64>,
}

#[tokio::main]
async fn main() {
    let mut app = Router::new(RouterConfig {
        route_prefix: "/api/v1".into(),
        ..Default::default()
    });

    app.middleware(async |req: Request, next: Next| {
        println!("middleware: before");
        let response = next.run(req).await?;
        println!("middleware: after");
        Ok(response)
    });

    app.get("/", async |_req| Response::new().send("shsh"));
    app.get("/users", async |req| {
        let pagination: Pagination = req.query()?;
        let page = pagination.page.unwrap_or(1);
        let per_page = pagination.per_page.unwrap_or(10);
        Ok(json!({ "page": page, "per_page": per_page }))
    });
    app.post("/users/", async |req| {
        let data: LoginPaylod = req.json()?;
        let name = data.email.split('@').next();
        let user = json!({
            "id": "2234",
            "email": data.email,
            "name": name
        });
        let cookie = Cookie::new("auth.token", "sub-id")
            .secure(false)
            .path("/")
            .same_site(yam_server::SameSite::Lax);

        let response = Response::new()
            .cookie(cookie)
            .status(yam_server::StatusCode::StatusCreated)
            .json(&user)?;
        Ok(response)
    });
    app.get("/users/{id}", async |req| {
        let id: u32 = req.param_as("id")?;
        Ok(json!({"id":id}))
    });
    let listener = TcpListener::bind("localhost:3000").await.unwrap();
    app.serve(listener).await.unwrap();
}

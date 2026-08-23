use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use yam_router::router::Router;
use yam_server::Response;

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
    let mut app = Router::new(Some(true));
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
        Ok(user)
    });
    app.get("/users/{id}", async |req| {
        let id: u32 = req.param_as("id")?;
        Ok(json!({"id":id}))
    });
    let listener = TcpListener::bind("localhost:3000").await.unwrap();
    app.serve(listener).await.unwrap();
}

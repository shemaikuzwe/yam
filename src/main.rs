use std::io::Error;

use tokio::net::{TcpListener, TcpStream};
use yam_router::router::Router;
use yam_server::{Request, Response};

#[tokio::main]
async fn main() {
    let mut app = Router::new();
    app.get("/", async |_, res| res.send("hello").await);
    app.get("/users", get_users);
    app.post("/users", async |req, res| {
        println!("{:?}", req);
        res.send("user created").await
    });
    let listener = TcpListener::bind("localhost:3000").await.unwrap();
    app.serve(listener).await.unwrap();
}

async fn get_users(_req: Request, res: Response<TcpStream>) -> Result<(), Error> {
    res.send("hello").await
}

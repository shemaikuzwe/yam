use std::io;

use tokio::net::{TcpListener, TcpStream};

use http_server::{
    request::Request,
    response::{Response, StatusCode},
    server::{HandlerFuture, Server},
};

fn app<'a>(req: &'a Request, res: &'a mut Response<&'a mut TcpStream>) -> HandlerFuture<'a> {
    Box::pin(async move {
        let target = match &req.request_line {
            Some(request_line) => request_line.request_target.as_str(),
            None => {
                return res
                    .status(StatusCode::StatusBadRequest)
                    .send("Bad request")
                    .await;
            }
        };

        match target {
            "/" => {
                res.status(StatusCode::StatusOk)
                    .send("Hello from Rust")
                    .await
            },
            "/users"=>{
              res.status(StatusCode::StatusOk)
                  .send("no users found").await  
            },
            _ => {
                res.status(StatusCode::StatusNotFound)
                    .send("Not found")
                    .await
            }
        }
    })
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    Server::serve(listener, app).await
}

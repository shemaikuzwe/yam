use std::{io, net::TcpStream};

use serde::Serialize;

use crate::response::{Response, StatusCode};

pub struct Server;

impl Server {
    pub fn run_server() {}
}
#[derive(Serialize)]
struct User {
    name: String,
    age: usize,
}
fn handle_request(stream: TcpStream) -> io::Result<()> {
    let mut res = Response::new(stream);
    let user = User {
        name: "Shema".to_string(),
        age: 20,
    };
    res.status(StatusCode::StatusOk).json(&user)
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yam_http::server::Cookie;

#[derive(Deserialize)]
pub struct Signup {
    pub email: String,
    pub password: String,
    pub name: String,
}
#[derive(Deserialize)]
pub struct Login {
    pub email: String,
    pub password: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub id: Uuid,
    pub sub: Uuid,
    pub email: String,
    pub name: String,
    pub exp: usize,
    pub iat: usize,
}
#[derive(Debug)]
pub struct AuthResponse {
    pub cookie: Cookie,
    pub token: String,
    pub payload: Payload,
}

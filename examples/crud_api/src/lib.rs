use std::env;

use diesel::{Connection, PgConnection};
use dotenv::dotenv;
pub mod admin;
pub mod auth;
pub mod middleware;
pub mod models;
pub mod schema;
pub mod shared;

pub fn connect_db() -> PgConnection {
    PgConnection::establish(&config().database_url)
        .unwrap_or_else(|_| panic!("error connecting to database"))
}
pub struct Env {
    pub database_url: String,
    pub jwt_secret: String,
    pub port: i64,
}

pub fn config() -> Env {
    dotenv().ok();
    Env {
        database_url: env::var("DATABASE_URL").expect("DATABASE_URL not set"),
        jwt_secret: env::var("JWT_SECRET").expect("JWT_SECRET not set"),
        port: env::var("PORT")
            .unwrap_or("3000".to_string())
            .parse::<i64>()
            .expect("Invalid PORT"),
    }
}

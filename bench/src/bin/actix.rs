use actix_web::{App, HttpServer, web};
use bench::{ADDR, HELLO, MESSAGE};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(async || HELLO))
            .route("/json", web::get().to(async || web::Json(MESSAGE)))
            .route(
                "/users/{id}",
                web::get().to(async |id: web::Path<String>| id.into_inner()),
            )
    })
    .bind(ADDR)?
    .run()
    .await
}

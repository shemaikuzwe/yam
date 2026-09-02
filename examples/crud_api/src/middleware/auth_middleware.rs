use yam_http::{
    router::Next,
    server::{HttpError, IntoResponse, Request, Response},
};

use crate::{auth::auth_service::verify_jwt, shared::AppError};

pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, HttpError> {
    let req_path = req.path();

    if req_path.contains("/auth") || req_path.contains("/health") {
        return next.run(req).await;
    }

    let Some(token) = get_token(&req) else {
        return Ok(AppError::UnAuthorized.into_response());
    };

    let user = match verify_jwt(token) {
        Ok(payload) => payload,
        Err(err) => return Ok(err.into_response()),
    };
    req.extensions_mut().insert(user);
    next.run(req).await
}
fn get_token(req: &Request) -> Option<String> {
    let cookie = req.cookie("auth.token").map(|c| c.to_string()).or_else(|| {
        req.headers
            .get("Authorization")
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string())
    });
    cookie
}

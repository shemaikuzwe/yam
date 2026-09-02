use crate::{
    auth::{
        auth_service,
        dtos::{Login, Signup},
    },
    shared::{ApiResponse, AppError},
};

use yam_http::server::{Request, Response};

pub async fn sign_up(req: Request) -> Result<Response, AppError> {
    let payload: Signup = req.json()?;
    let result = auth_service::signup(payload).await?;
    let res = Response::new()
        .status(yam_http::server::StatusCode::StatusCreated)
        .cookie(result.cookie)
        .json(&ApiResponse {
            data: Some(result.payload),
            success: true,
            message: result.token,
        });
    Ok(res)
}

pub async fn login(req: Request) -> Result<Response, AppError> {
    let payload: Login = req.json().unwrap();
    let result = auth_service::login(payload).await?;
    let res = Response::new().cookie(result.cookie).json(&ApiResponse {
        success: true,
        message: result.token,
        data: Some(result.payload),
    });
    Ok(res)
}

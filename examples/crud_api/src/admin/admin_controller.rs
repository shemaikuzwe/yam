use uuid::Uuid;
use yam_http::server::{Json, Request};

use crate::{
    admin::{admin_service, dtos::UpdateDto},
    models::UserResponse,
    shared::{ApiResponse, AppError},
};

pub async fn get_users(_req: Request) -> Result<Json<ApiResponse<Vec<UserResponse>>>, AppError> {
    let users = admin_service::get_users().await?;
    Ok(Json(ApiResponse {
        success: true,
        message: String::from("users fetched successfully"),
        data: Some(users),
    }))
}

pub async fn get_user(req: Request) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let id: Uuid = req.param_as("id")?;
    let user = admin_service::get_user_by_id(id).await?;
    Ok(Json(ApiResponse {
        success: true,
        message: String::from("user fetched successfully"),
        data: Some(user),
    }))
}
pub async fn update_user(req: Request) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let id: Uuid = req.param_as("id")?;
    let payload: UpdateDto = req.json()?;
    let user = admin_service::update_user(id, payload).await?;
    Ok(Json(ApiResponse {
        success: true,
        message: String::from("user updated successfully"),
        data: Some(user),
    }))
}

pub async fn delete_user(req: Request) -> Result<Json<ApiResponse<String>>, AppError> {
    let id: Uuid = req.param_as("id")?;
    let result = admin_service::delete_user(id).await?;
    Ok(Json(ApiResponse {
        success: true,
        message: String::from("user deleted"),
        data: Some(result),
    }))
}

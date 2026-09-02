use std::time::Duration;

use crate::{
    auth::dtos::{AuthResponse, Login, Payload, Signup},
    config, connect_db,
    models::{User, UserSelect},
    schema::users::{self, dsl::*},
    shared::AppError,
};
use bcrypt::DEFAULT_COST;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, SelectableHelper};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use tracing::error;
use yam_http::server::Cookie;

pub async fn login(payload: Login) -> Result<AuthResponse, AppError> {
    let mut conn = connect_db();
    let result = users
        .filter(email.eq(payload.email))
        .select(UserSelect::as_select())
        .first(&mut conn)
        .map_err(|err| match err {
            diesel::result::Error::NotFound => AppError::InvalidCredentials,
            other => AppError::Database(other),
        })?;
    let is_valid = bcrypt::verify(payload.password, &result.password)?;
    if !is_valid {
        return Err(AppError::InvalidCredentials);
    }
    let payload = result.into();
    let token = sign_jwt(&payload)?;
    let cookie = set_cookie(token.as_str());
    Ok(AuthResponse {
        cookie,
        token,
        payload,
    })
}

pub async fn signup(payload: Signup) -> Result<AuthResponse, AppError> {
    let mut conn = connect_db();
    let hash_password = bcrypt::hash(&payload.password, DEFAULT_COST)?;

    let user = User {
        email: payload.email,
        name: payload.name,
        password: hash_password,
    };
    let result = diesel::insert_into(users::table)
        .values(&user)
        .returning(UserSelect::as_returning())
        .get_result(&mut conn)?;
    let payload = result.into();
    let token = sign_jwt(&payload)?;
    let cookie = set_cookie(token.as_str());

    Ok(AuthResponse {
        cookie,
        token,
        payload,
    })
}

fn sign_jwt(payload: &Payload) -> Result<String, AppError> {
    let jwt_secret = config().jwt_secret;

    let token = encode(
        &Header::default(),
        &payload,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )
    .map_err(|err| {
        error!("error: {:?}", err);
        AppError::InternalServerError
    })?;
    Ok(token)
}
pub fn verify_jwt(token: String) -> Result<Payload, AppError> {
    let jwt_secret = config().jwt_secret;
    let payload = decode::<Payload>(
        &token,
        &DecodingKey::from_secret(jwt_secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|err| {
        error!("error: {:?}", err);
        AppError::InvalidToken
    })?
    .claims;
    Ok(payload)
}
fn set_cookie(token: &str) -> Cookie {
    Cookie::new("auth.token", token.to_string())
        .path("/")
        .secure(false)
        .same_site(yam_http::server::SameSite::Lax)
        .max_age(Duration::from_hours(24))
}

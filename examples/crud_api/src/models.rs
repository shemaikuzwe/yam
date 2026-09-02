use crate::{auth::dtos::Payload, schema::users};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Queryable, Selectable, Insertable, Serialize)]
#[diesel(table_name=users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Queryable, Selectable, Insertable, Serialize)]
#[diesel(table_name=users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UserSelect {
    //fields should be correctly arranged as defined in schema
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub password: String,
}
impl From<UserSelect> for Payload {
    fn from(user: UserSelect) -> Self {
        let now = Utc::now();
        Payload {
            id: user.id,
            sub: user.id,
            email: user.email,
            name: user.name,
            exp: (now + Duration::hours(24)).timestamp() as usize,
            iat: now.timestamp() as usize,
        }
    }
}

#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name=users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

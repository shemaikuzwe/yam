use diesel::query_builder::AsChangeset;
use serde::Deserialize;

use crate::schema::users;

#[derive(Debug, Deserialize, AsChangeset)]
#[diesel(table_name=users)]
pub struct UpdateDto {
    pub name: Option<String>,
    pub email: Option<String>,
}

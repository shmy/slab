use shared_contract::value_object::{id::ID, phone_number::PhoneNumber};
use sqlx::prelude::FromRow;

use crate::value_object::hashed_password::HashedPassword;

#[derive(Debug, Clone, FromRow)]
pub struct Account {
    pub id: ID,
    pub name: String,
    pub phone: PhoneNumber,
    pub password: HashedPassword,
    pub privileged: bool,
    pub version: i64,
}

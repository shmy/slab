use serde::Serialize;
use shared_contract::value_object::{id::ID, phone_number::PhoneNumber};
use sqlx::prelude::FromRow;

use crate::value_object::hashed_password::HashedPassword;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Account {
    pub id: ID,
    pub name: String,
    pub phone: PhoneNumber,
    /// 密码哈希不进任何序列化输出（审计快照亦排除）
    #[serde(skip)]
    pub password: HashedPassword,
    pub privileged: bool,
    /// 乐观锁版本号不进序列化输出（每次更新递增，进审计属于噪音）
    #[serde(skip)]
    pub version: i64,
}

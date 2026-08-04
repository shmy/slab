use serde::{Deserialize, Serialize};
use shared_contract::event::Event;
use shared_contract::value_object::id::ID;

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountCreatedEvent {
    pub id: ID,
}
impl Event for AccountCreatedEvent {
    const TOPIC: &'static str = "slab.account.created";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountLoggedInEvent {
    pub id: ID,
}
impl Event for AccountLoggedInEvent {
    const TOPIC: &'static str = "slab.account.logged_in";
}

use crate::TokenHelper;
use std::fmt::Debug;

#[derive(Clone)]
pub struct TokenBundle {
    customer: TokenHelper,
    account: TokenHelper,
}

impl Debug for TokenBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenBundle").finish()
    }
}

impl TokenBundle {
    pub fn new(customer: TokenHelper, account: TokenHelper) -> Self {
        Self { customer, account }
    }

    pub fn customer(&self) -> &TokenHelper {
        &self.customer
    }

    pub fn account(&self) -> &TokenHelper {
        &self.account
    }
}

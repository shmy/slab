use std::ops::Deref;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenRealm {
    Customer,
    Account,
}

impl Deref for TokenRealm {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        match self {
            TokenRealm::Customer => "customer",
            TokenRealm::Account => "account",
        }
    }
}

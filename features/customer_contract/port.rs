use rootcause::Result;
use shared_contract::value_object::id::ID;
use shared_contract::value_object::phone_number::PhoneNumber;
use sqlx::PgConnection;

use crate::entity::Customer;

pub struct CustomerPort;

impl CustomerPort {
    pub async fn by_id(conn: &mut PgConnection, id: &ID) -> Result<Option<Customer>> {
        let row = sqlx::query!(
            r#"SELECT id, code, name, contact_person,
                      phone as "phone: PhoneNumber",
                      address, payment_terms, is_active
               FROM customers WHERE id = $1"#,
            id as _
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|r| Customer {
            id: ID::new_unchecked(r.id),
            code: r.code,
            name: r.name,
            contact_person: r.contact_person,
            phone: r.phone,
            address: r.address,
            payment_terms: r.payment_terms,
            is_active: r.is_active,
        }))
    }
}

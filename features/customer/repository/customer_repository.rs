use customer_contract::entity::Customer;
use customer_contract::error::CustomerError;
use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;

use crate::endpoint::customer_update::UpdateCustomerRequest;

pub struct CustomerRepository;

impl CustomerRepository {
    pub async fn create(conn: &mut PgConnection, customer: &Customer) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO customers (id, code, name, contact_person, phone, address, payment_terms, is_active)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &*customer.id,
            customer.code,
            customer.name,
            customer.contact_person,
            customer.phone.as_ref().map(|p| p.as_str()),
            customer.address,
            customer.payment_terms,
            customer.is_active,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn update(
        conn: &mut PgConnection,
        id: &ID,
        request: &UpdateCustomerRequest,
    ) -> Result<bool> {
        let current = sqlx::query!(
            r#"SELECT name, contact_person, phone, address, payment_terms, is_active
               FROM customers WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(CustomerError::NotFound)?;

        let name = request.name.as_deref().unwrap_or(&current.name);
        let contact_person: Option<&str> = match &request.contact_person {
            Some(Some(v)) => Some(v.as_str()),
            Some(None) => None,
            None => current.contact_person.as_deref(),
        };
        let phone: Option<&str> = match &request.phone {
            Some(Some(v)) => Some(v.as_str()),
            Some(None) => None,
            None => current.phone.as_deref(),
        };
        let address: Option<&str> = match &request.address {
            Some(Some(v)) => Some(v.as_str()),
            Some(None) => None,
            None => current.address.as_deref(),
        };
        let payment_terms: Option<&str> = match &request.payment_terms {
            Some(Some(v)) => Some(v.as_str()),
            Some(None) => None,
            None => current.payment_terms.as_deref(),
        };
        let is_active = request.is_active.unwrap_or(current.is_active);

        sqlx::query!(
            r#"UPDATE customers SET name = $1, contact_person = $2, phone = $3,
                address = $4, payment_terms = $5, is_active = $6 WHERE id = $7"#,
            name,
            contact_person,
            phone,
            address,
            payment_terms,
            is_active,
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(true)
    }

    pub async fn delete(conn: &mut PgConnection, id: &ID) -> Result<bool> {
        let affected = sqlx::query!(
            "UPDATE customers SET is_active = FALSE WHERE id = $1",
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(affected.rows_affected() > 0)
    }
}

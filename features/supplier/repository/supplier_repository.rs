use rootcause::Result;
use shared_contract::value_object::id::ID;
use sqlx::PgConnection;
use supplier_contract::entity::Supplier;
use supplier_contract::error::SupplierError;

use crate::endpoint::supplier_update::UpdateSupplierRequest;

pub struct SupplierRepository;

impl SupplierRepository {
    pub async fn create(conn: &mut PgConnection, supplier: &Supplier) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO suppliers (id, code, name, contact_person, phone, address, payment_terms, is_active)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &*supplier.id,
            supplier.code,
            supplier.name,
            supplier.contact_person,
            supplier.phone.as_ref().map(|p| p.as_str()),
            supplier.address,
            supplier.payment_terms,
            supplier.is_active,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    pub async fn update(
        conn: &mut PgConnection,
        id: &ID,
        request: &UpdateSupplierRequest,
    ) -> Result<bool> {
        let current = sqlx::query!(
            r#"SELECT name, contact_person, phone, address, payment_terms, is_active
               FROM suppliers WHERE id = $1"#,
            id as _
        )
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(SupplierError::NotFound)?;

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
            r#"UPDATE suppliers SET name = $1, contact_person = $2, phone = $3,
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
            "UPDATE suppliers SET is_active = FALSE WHERE id = $1",
            id as _
        )
        .execute(&mut *conn)
        .await?;
        Ok(affected.rows_affected() > 0)
    }
}

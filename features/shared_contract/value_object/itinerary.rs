use crate::value_object::money::Money;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, Postgres, Type};
use utoipa::ToSchema;

/// 行程
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({"days": []}))]
pub struct Itinerary {
    pub days: Vec<DayPlan>,
}

impl Itinerary {
    pub fn base_price(&self) -> Money {
        self.days
            .iter()
            .flat_map(|day| &day.items)
            .map(|item| item.total_price())
            .sum()
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DayPlan {
    pub day: u32,
    pub title: Option<String>,
    pub items: Vec<ItineraryItem>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ItineraryItem {
    Hotel {
        name: String,
        unit_price: Money,
        quantity: u32,
        days: u32,
    },

    Vehicle {
        name: String,
        unit_price: Money,
    },

    Attraction {
        name: String,
        unit_price: Money,
        quantity: u32,
    },

    Custom {
        name: String,
        unit_price: Money,
    },
}

impl ItineraryItem {
    pub fn total_price(&self) -> Money {
        match self {
            ItineraryItem::Hotel {
                unit_price,
                quantity,
                days,
                ..
            } => *unit_price * (*quantity as i64) * (*days as i64),

            ItineraryItem::Vehicle { unit_price, .. } => *unit_price,

            ItineraryItem::Attraction {
                unit_price,
                quantity,
                ..
            } => *unit_price * (*quantity as i64),

            ItineraryItem::Custom { unit_price, .. } => *unit_price,
        }
    }
}

impl Type<Postgres> for Itinerary {
    fn type_info() -> PgTypeInfo {
        <sqlx::types::Json<serde_json::Value> as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <sqlx::types::Json<serde_json::Value> as Type<Postgres>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Postgres> for Itinerary {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let json = <sqlx::types::Json<serde_json::Value> as Decode<Postgres>>::decode(value)?;
        Ok(serde_json::from_value(json.0)?)
    }
}

impl<'q> Encode<'q, Postgres> for Itinerary {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let v = serde_json::to_value(self)?;
        <sqlx::types::Json<serde_json::Value> as Encode<Postgres>>::encode(
            sqlx::types::Json(v),
            buf,
        )
    }
}

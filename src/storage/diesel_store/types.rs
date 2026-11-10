//! Column types that both databases can store, each in its own way.
//!
//! | SQL type      | Postgres      | SQLite                                  |
//! | ------------- | ------------- | --------------------------------------- |
//! | `DbUuid`      | `uuid`        | text, hyphenated                        |
//! | `DbTimestamp` | `timestamptz` | text, UTC with microseconds             |
//! | `DbMoney`     | `numeric`     | integer, in ten-thousandths of a unit   |
//! | `DbJson`      | `jsonb`       | text                                    |
//! | enums         | native enum   | text                                    |

use std::io::Write;

use chrono::{DateTime, Utc};
use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow, Queryable};
use diesel::expression::AsExpression;
// What `#[derive(AsExpression)]` expands to. The enums cannot derive it without the models
// depending on Diesel.
use diesel::internal::derives::as_expression::Bound;
use diesel::pg::{Pg, PgValue};
use diesel::query_builder::QueryId;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{self, HasSqlType, Nullable, SqlType};
use diesel::sqlite::{Sqlite, SqliteValue};
use rust_decimal::{Decimal, RoundingStrategy};
use uuid::Uuid;

use super::connection::{MultiBackend, MultiRawValue};
use crate::models::{
    account::{AccountType, NormalBalance},
    transaction::EntryType,
};

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(oid = 2950, array_oid = 2951))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbUuid;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(oid = 1184, array_oid = 1185))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbTimestamp;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(oid = 1700, array_oid = 1231))]
#[diesel(sqlite_type(name = "Long"))]
pub struct DbMoney;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(oid = 3802, array_oid = 3807))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbJson;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(name = "account_type"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbAccountType;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(name = "normal_balance"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbNormalBalance;

#[derive(SqlType, QueryId, Debug, Clone, Copy, Default)]
#[diesel(postgres_type(name = "entry_type"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct DbEntryType;

impl sql_types::Foldable for DbMoney {
    type Sum = Nullable<DbMoney>;
    type Avg = Nullable<DbMoney>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsExpression, FromSqlRow)]
#[diesel(sql_type = DbUuid)]
pub struct Id(pub Uuid);

impl Id {
    pub fn new() -> Self {
        Id(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = DbTimestamp)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Timestamp(Utc::now())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = DbMoney)]
pub struct Money(pub Decimal);

#[derive(Debug, Clone, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = DbJson)]
pub struct Json(pub serde_json::Value);

// Text sorts in time order only if every value has the same shape.
const SQLITE_TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6fZ";
// Matches the scale of the Postgres `NUMERIC(19,4)` columns.
const MONEY_SCALE: u32 = 4;

macro_rules! on_multi_backend {
    ($sql_type:ty => $rust_type:ty) => {
        impl HasSqlType<$sql_type> for MultiBackend {
            fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
                MultiBackend::lookup_sql_type::<$sql_type>(lookup)
            }
        }

        impl ToSql<$sql_type, MultiBackend> for $rust_type {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, MultiBackend>) -> serialize::Result {
                out.set_value((<$sql_type>::default(), self));
                Ok(IsNull::No)
            }
        }

        impl FromSql<$sql_type, MultiBackend> for $rust_type {
            fn from_sql(value: MultiRawValue<'_>) -> deserialize::Result<Self> {
                value.from_sql::<Self, $sql_type>()
            }
        }
    };
}

on_multi_backend!(DbUuid => Id);
on_multi_backend!(DbTimestamp => Timestamp);
on_multi_backend!(DbMoney => Money);
on_multi_backend!(DbJson => Json);

impl ToSql<DbUuid, Pg> for Id {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <Uuid as ToSql<sql_types::Uuid, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<DbUuid, Pg> for Id {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        <Uuid as FromSql<sql_types::Uuid, Pg>>::from_sql(value).map(Id)
    }
}

impl ToSql<DbUuid, Sqlite> for Id {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<DbUuid, Sqlite> for Id {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let text = <String as FromSql<sql_types::Text, Sqlite>>::from_sql(value)?;
        Ok(Id(Uuid::parse_str(&text)?))
    }
}

impl ToSql<DbTimestamp, Pg> for Timestamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <DateTime<Utc> as ToSql<sql_types::Timestamptz, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<DbTimestamp, Pg> for Timestamp {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        <DateTime<Utc> as FromSql<sql_types::Timestamptz, Pg>>::from_sql(value).map(Timestamp)
    }
}

impl ToSql<DbTimestamp, Sqlite> for Timestamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.format(SQLITE_TIMESTAMP_FORMAT).to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<DbTimestamp, Sqlite> for Timestamp {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let text = <String as FromSql<sql_types::Text, Sqlite>>::from_sql(value)?;
        Ok(Timestamp(
            DateTime::parse_from_rfc3339(&text)?.with_timezone(&Utc),
        ))
    }
}

impl ToSql<DbMoney, Pg> for Money {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <Decimal as ToSql<sql_types::Numeric, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<DbMoney, Pg> for Money {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        <Decimal as FromSql<sql_types::Numeric, Pg>>::from_sql(value).map(Money)
    }
}

impl ToSql<DbMoney, Sqlite> for Money {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        // Rounds the way Postgres does when it stores into NUMERIC(19,4).
        let mut scaled = self
            .0
            .round_dp_with_strategy(MONEY_SCALE, RoundingStrategy::MidpointAwayFromZero);
        scaled.rescale(MONEY_SCALE);
        out.set_value(i64::try_from(scaled.mantissa())?);
        Ok(IsNull::No)
    }
}

impl FromSql<DbMoney, Sqlite> for Money {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let units = <i64 as FromSql<sql_types::BigInt, Sqlite>>::from_sql(value)?;
        Ok(Money(Decimal::new(units, MONEY_SCALE)))
    }
}

impl ToSql<DbJson, Pg> for Json {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <serde_json::Value as ToSql<sql_types::Jsonb, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<DbJson, Pg> for Json {
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        <serde_json::Value as FromSql<sql_types::Jsonb, Pg>>::from_sql(value).map(Json)
    }
}

impl ToSql<DbJson, Sqlite> for Json {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(IsNull::No)
    }
}

impl FromSql<DbJson, Sqlite> for Json {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let text = <String as FromSql<sql_types::Text, Sqlite>>::from_sql(value)?;
        Ok(Json(serde_json::from_str(&text)?))
    }
}

macro_rules! text_enum {
    ($sql_type:ty => $rust_type:ty) => {
        on_multi_backend!($sql_type => $rust_type);

        impl ToSql<$sql_type, Pg> for $rust_type {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
                out.write_all(self.as_str().as_bytes())?;
                Ok(IsNull::No)
            }
        }

        impl FromSql<$sql_type, Pg> for $rust_type {
            fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
                Ok(std::str::from_utf8(value.as_bytes())?.parse()?)
            }
        }

        impl ToSql<$sql_type, Sqlite> for $rust_type {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                out.set_value(self.as_str());
                Ok(IsNull::No)
            }
        }

        impl FromSql<$sql_type, Sqlite> for $rust_type {
            fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
                let text = <String as FromSql<sql_types::Text, Sqlite>>::from_sql(value)?;
                Ok(text.parse()?)
            }
        }

        impl AsExpression<$sql_type> for $rust_type {
            type Expression = Bound<$sql_type, Self>;

            fn as_expression(self) -> Self::Expression {
                Bound::new(self)
            }
        }

        impl<DB> Queryable<$sql_type, DB> for $rust_type
        where
            DB: Backend,
            Self: FromSql<$sql_type, DB>,
        {
            type Row = Self;

            fn build(row: Self) -> deserialize::Result<Self> {
                Ok(row)
            }
        }
    };
}

text_enum!(DbAccountType => AccountType);
text_enum!(DbNormalBalance => NormalBalance);
text_enum!(DbEntryType => EntryType);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_timestamps_sort_as_text_in_time_order() {
        let earlier = "2026-03-04T05:06:07Z".parse::<DateTime<Utc>>().unwrap();
        let later = earlier + chrono::Duration::milliseconds(500);

        let earlier_text = earlier.format(SQLITE_TIMESTAMP_FORMAT).to_string();
        let later_text = later.format(SQLITE_TIMESTAMP_FORMAT).to_string();

        assert_eq!(earlier_text, "2026-03-04T05:06:07.000000Z");
        assert!(earlier_text < later_text);
    }
}

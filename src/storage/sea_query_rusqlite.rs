//! Custom SQLite parameter binder bridging `sea_query` with `rusqlite`.
//!
//! Adapted from upstream `sea-query-rusqlite`:
//! <https://github.com/SeaQL/sea-query/blob/master/sea-query-rusqlite/src/lib.rs>
//!
//! ### Rationale
//! Upstream `sea-query-rusqlite` usually lags behind new `rusqlite` releases, constraining dependency updates.
//! Since the integration is very small, inlining this adapter avoids being blocked on upstream release cycles
//! and is much easier to maintain than keeping an updated external fork.

use rusqlite::{
    Result, ToSql,
    types::{Null, ToSqlOutput},
};
use sea_query::{DeleteStatement, InsertStatement, QueryBuilder, SelectStatement, UpdateStatement, Value, WithQuery};

#[derive(Clone, Debug, PartialEq)]
pub struct RusqliteValue(pub Value);

#[derive(Clone, Debug, PartialEq)]
pub struct RusqliteValues(pub Vec<RusqliteValue>);

impl RusqliteValues {
    pub fn as_params(&self) -> Vec<&dyn ToSql> {
        self.0.iter().map(|x| x as &dyn ToSql).collect()
    }
}

pub trait RusqliteBinder {
    fn build_rusqlite<T: QueryBuilder>(&self, query_builder: T) -> (String, RusqliteValues);
}

macro_rules! impl_rusqlite_binder {
    ($l:ident) => {
        impl RusqliteBinder for $l {
            fn build_rusqlite<T: QueryBuilder>(&self, query_builder: T) -> (String, RusqliteValues) {
                let (query, values) = self.build(query_builder);
                (
                    query,
                    RusqliteValues(values.into_iter().map(RusqliteValue).collect()),
                )
            }
        }
    };
}

impl_rusqlite_binder!(SelectStatement);
impl_rusqlite_binder!(UpdateStatement);
impl_rusqlite_binder!(InsertStatement);
impl_rusqlite_binder!(DeleteStatement);
impl_rusqlite_binder!(WithQuery);

impl ToSql for RusqliteValue {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
        macro_rules! opt_string_to_sql {
            ($v:expr) => {
                match $v {
                    Some(v) => Ok(ToSqlOutput::from(v)),
                    None => Null.to_sql(),
                }
            };
        }

        match &self.0 {
            Value::Bool(v) => v.to_sql(),
            Value::TinyInt(v) => v.to_sql(),
            Value::SmallInt(v) => v.to_sql(),
            Value::Int(v) => v.to_sql(),
            Value::BigInt(v) => v.to_sql(),
            Value::TinyUnsigned(v) => v.to_sql(),
            Value::SmallUnsigned(v) => v.to_sql(),
            Value::Unsigned(v) => v.to_sql(),
            Value::BigUnsigned(v) => match v {
                Some(val) => {
                    let i = i64::try_from(*val).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    Ok(ToSqlOutput::from(i))
                }
                None => Null.to_sql(),
            },
            Value::Float(v) => v.to_sql(),
            Value::Double(v) => v.to_sql(),
            Value::String(v) => match v {
                Some(v) => v.as_str().to_sql(),
                None => Null.to_sql(),
            },
            Value::Char(v) => opt_string_to_sql!(v.map(|c| c.to_string())),
            Value::Bytes(v) => match v {
                Some(v) => v.as_slice().to_sql(),
                None => Null.to_sql(),
            },
            Value::Enum(e) => match e {
                sea_query::OptionEnum::Some(e) => e.value.as_ref().to_sql(),
                sea_query::OptionEnum::None(_) => Null.to_sql(),
            },
        }
    }
}

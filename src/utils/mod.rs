use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Column, Row};
use uuid::Uuid;

pub(crate) fn json_rows(rows: Vec<sqlx::mysql::MySqlRow>) -> Value {
    Value::Array(rows.into_iter().map(row_to_json).collect())
}

pub(crate) fn row_to_json(row: sqlx::mysql::MySqlRow) -> Value {
    let mut value = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name();
        let item = if let Ok(v) = row.try_get::<Uuid, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<String, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<bool, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<i32, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<i64, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<f64, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<DateTime<Utc>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Value, _>(name) {
            v
        } else if let Ok(v) = row.try_get::<Option<String>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Option<Uuid>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Option<i32>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Option<i64>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Option<f64>, _>(name) {
            json!(v)
        } else if let Ok(v) = row.try_get::<Option<DateTime<Utc>>, _>(name) {
            json!(v)
        } else {
            Value::Null
        };
        value.insert(name.to_string(), item);
    }
    Value::Object(value)
}

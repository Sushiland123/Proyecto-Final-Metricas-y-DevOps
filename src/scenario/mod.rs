//! ESC3 domain, durable reception, query API and independently stoppable processor.
pub mod http;
pub mod store;

use crate::{
    domain::{Validate, Window, identifier},
    error::Error,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub temperature_min: f64,
    pub temperature_max: f64,
    pub humidity_min: f64,
    pub humidity_max: f64,
}
impl Validate for Limits {
    fn validate(&self) -> Result<(), Error> {
        if self.temperature_min.is_finite()
            && self.temperature_max.is_finite()
            && self.humidity_min.is_finite()
            && self.humidity_max.is_finite()
            && -80.0 <= self.temperature_min
            && self.temperature_min < self.temperature_max
            && self.temperature_max <= 150.0
            && 0.0 <= self.humidity_min
            && self.humidity_min < self.humidity_max
            && self.humidity_max <= 100.0
        {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceCreate {
    pub id: String,
    pub facility_id: String,
    pub limits: Limits,
}
impl Validate for DeviceCreate {
    fn validate(&self) -> Result<(), Error> {
        if !identifier(&self.id) || !identifier(&self.facility_id) {
            return Err(Error::InvalidInput);
        }
        self.limits.validate()
    }
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Device {
    pub id: String,
    pub facility_id: String,
    pub temperature_min: f64,
    pub temperature_max: f64,
    pub humidity_min: f64,
    pub humidity_max: f64,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingCreate {
    pub event_id: String,
    pub device_id: String,
    /// RFC3339 with explicit timezone, normalized to UTC.
    pub occurred_at: DateTime<Utc>,
    /// Declared physical validity range, distinct from configurable alert limits.
    pub temperature: f64,
    pub humidity: f64,
    /// running | stopped | fault
    pub state: String,
}
impl Validate for ReadingCreate {
    fn validate(&self) -> Result<(), Error> {
        if !identifier(&self.device_id)
            || self.event_id.is_empty()
            || self.event_id.chars().count() > 128
            || self.event_id.contains('\0')
            || !self.temperature.is_finite()
            || !(-80.0..=150.0).contains(&self.temperature)
            || !self.humidity.is_finite()
            || !(0.0..=100.0).contains(&self.humidity)
            || !matches!(self.state.as_str(), "running" | "stopped" | "fault")
        {
            Err(Error::InvalidInput)
        } else {
            Ok(())
        }
    }
}
#[derive(Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Reading {
    pub id: Uuid,
    pub device_id: String,
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub temperature: f64,
    pub humidity: f64,
    pub state: String,
    pub processed_at: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize, ToSchema)]
pub struct Receipt {
    pub id: Uuid,
    pub reading_id: Option<Uuid>,
    pub outcome: String,
    pub reason: Option<String>,
}
#[derive(Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default = "page_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub device_id: Option<String>,
}
fn page_limit() -> i64 {
    100
}
impl Validate for Page {
    fn validate(&self) -> Result<(), Error> {
        if !(1..=1000).contains(&self.limit)
            || !(0..=100_000).contains(&self.offset)
            || self.device_id.as_ref().is_some_and(|id| !identifier(id))
        {
            Err(Error::InvalidInput)
        } else {
            Ok(())
        }
    }
}
#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct Period {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    #[serde(default = "page_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
impl Validate for Period {
    fn validate(&self) -> Result<(), Error> {
        Window {
            from: self.from,
            to: self.to,
        }
        .validate()?;
        Page {
            limit: self.limit,
            offset: self.offset,
            device_id: None,
        }
        .validate()
    }
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Aggregate {
    pub count: i64,
    pub temperature_min: Option<f64>,
    pub temperature_max: Option<f64>,
    pub temperature_avg: Option<f64>,
    pub humidity_min: Option<f64>,
    pub humidity_max: Option<f64>,
    pub humidity_avg: Option<f64>,
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Counters {
    pub received: i64,
    /// Includes format-valid retransmissions; unique validated = accepted.
    pub validated: i64,
    pub accepted: i64,
    pub duplicates: i64,
    pub rejected: i64,
    pub processed: i64,
    pub pending: i64,
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Alert {
    pub id: Uuid,
    pub reading_id: Uuid,
    pub device_id: String,
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub kind: String,
    pub value: f64,
    pub threshold: f64,
    pub created_at: DateTime<Utc>,
}

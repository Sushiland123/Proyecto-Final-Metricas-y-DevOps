//! Wire contracts and domain validation, independent of HTTP and SQL adapters.
use crate::error::Error;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Accept a nonempty ASCII identifier of at most 64 bytes, without separators used in paths.
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub trait Validate {
    /// Check semantic invariants; failures must not contain submitted values.
    fn validate(&self) -> Result<(), Error>;
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EventCreate {
    #[schema(
        min_length = 1,
        max_length = 64,
        pattern = "^[A-Za-z0-9][A-Za-z0-9_-]*$"
    )]
    pub source: String,
    #[schema(min_length = 1, max_length = 128)]
    pub external_id: String,
    #[serde(rename = "type")]
    #[schema(
        min_length = 1,
        max_length = 64,
        pattern = "^[A-Za-z0-9][A-Za-z0-9_-]*$"
    )]
    pub event_type: String,
    /// RFC3339 with explicit timezone; serialized in UTC.
    pub occurred_at: DateTime<Utc>,
    /// JSON object, at most 16384 bytes serialized compactly in UTF-8.
    pub payload: Map<String, Value>,
}

/// PostgreSQL text and JSONB cannot represent U+0000, including nested keys.
fn contains_nul(value: &Value) -> bool {
    match value {
        Value::String(value) => value.contains('\0'),
        Value::Array(values) => values.iter().any(contains_nul),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.contains('\0') || contains_nul(value)),
        _ => false,
    }
}

impl Validate for EventCreate {
    /// Enforce identifier, PostgreSQL text/JSONB and serialized payload bounds before persistence.
    fn validate(&self) -> Result<(), Error> {
        if !identifier(&self.source)
            || !identifier(&self.event_type)
            || self.external_id.is_empty()
            || self.external_id.chars().count() > 128
            || self.external_id.contains('\0')
            || self
                .payload
                .iter()
                .any(|(key, value)| key.contains('\0') || contains_nul(value))
            || serde_json::to_vec(&self.payload)
                .map_err(|_| Error::InvalidInput)?
                .len()
                > 16 * 1024
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EventBatchCreate {
    #[schema(min_items = 1, max_items = 100)]
    pub events: Vec<EventCreate>,
}
impl Validate for EventBatchCreate {
    /// Validate every event and reject empty or oversized batches without writing partial data.
    fn validate(&self) -> Result<(), Error> {
        if self.events.is_empty() || self.events.len() > 100 {
            return Err(Error::InvalidInput);
        }
        self.events.iter().try_for_each(Validate::validate)
    }
}
#[derive(Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Event {
    pub id: Uuid,
    pub source: String,
    pub external_id: String,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub payload: Value,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}
impl Validate for Window {
    /// Require a strictly increasing half-open interval; timestamps already carry UTC semantics.
    fn validate(&self) -> Result<(), Error> {
        if self.from < self.to {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Type,
    Source,
}
impl Dimension {
    /// Only static, allow-listed SQL identifiers may be interpolated.
    pub fn column(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Source => "source",
        }
    }
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Minute,
    #[default]
    Hour,
    Day,
}
impl Bucket {
    /// Return the allow-listed PostgreSQL date_trunc unit, never a client-supplied SQL fragment.
    pub fn name(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }
    /// Limit requested series resolution to bounded minute, hour and day windows.
    pub fn max_days(self) -> i64 {
        match self {
            Self::Minute => 7,
            Self::Hour => 90,
            Self::Day => 366,
        }
    }
}
#[derive(Clone, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct CountsQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub group_by: Dimension,
}
impl CountsQuery {
    /// Extract the normalized interval shared by the query and response contract.
    pub fn window(&self) -> Window {
        Window {
            from: self.from,
            to: self.to,
        }
    }
}
impl Validate for CountsQuery {
    /// Check temporal ordering after Serde has restricted grouping to supported dimensions.
    fn validate(&self) -> Result<(), Error> {
        self.window().validate()
    }
}
#[derive(Clone, Default, Serialize, ToSchema)]
pub struct Filters {
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
}
impl Validate for Filters {
    /// Apply the same identifier rules to optional read filters as to ingested events.
    fn validate(&self) -> Result<(), Error> {
        if self
            .source
            .iter()
            .chain(self.event_type.iter())
            .all(|s| identifier(s))
        {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
/// Default page size used when the caller omits limit.
fn default_limit() -> i64 {
    50
}
/// Default top-level JSONB field used for numeric statistics.
fn default_numeric_field() -> String {
    "value".to_owned()
}
#[derive(Clone, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100, default = 50)]
    pub limit: i64,
    #[serde(default)]
    #[param(minimum = 0, maximum = 100000, default = 0)]
    pub offset: i64,
}
#[derive(Clone, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct TimeseriesQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    #[serde(default)]
    pub bucket: Bucket,
}
#[derive(Clone, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct SummaryQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    #[serde(default = "default_numeric_field")]
    pub numeric_field: String,
}
macro_rules! filtered_query {
    ($($query:ty),+) => { $(impl $query {
        /// Return the normalized query interval without altering its half-open semantics.
        pub fn window(&self) -> Window { Window { from: self.from, to: self.to } }
        /// Return the applied source/type filters for storage and response serialization.
        pub fn filters(&self) -> Filters { Filters { source: self.source.clone(), event_type: self.event_type.clone() } }
    })+ };
}
filtered_query!(ListQuery, TimeseriesQuery, SummaryQuery);
impl Validate for ListQuery {
    /// Validate filters, interval and bounded offset pagination before repository access.
    fn validate(&self) -> Result<(), Error> {
        self.window().validate()?;
        self.filters().validate()?;
        if (1..=100).contains(&self.limit) && (0..=100_000).contains(&self.offset) {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
impl Validate for TimeseriesQuery {
    /// Enforce filters and the maximum interval associated with the selected resolution.
    fn validate(&self) -> Result<(), Error> {
        self.window().validate()?;
        self.filters().validate()?;
        if self.to - self.from <= chrono::Duration::days(self.bucket.max_days()) {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
impl Validate for SummaryQuery {
    /// Validate interval, filters and the JSONB numeric-field identifier.
    fn validate(&self) -> Result<(), Error> {
        self.window().validate()?;
        self.filters().validate()?;
        if identifier(&self.numeric_field) {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct BatchResponse {
    pub inserted: usize,
    pub events: Vec<Event>,
}
#[derive(Serialize, ToSchema)]
pub struct ListResponse {
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub window: Window,
    pub filters: Filters,
    pub items: Vec<Event>,
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Count {
    pub key: String,
    pub count: i64,
}
#[derive(Serialize, ToSchema)]
pub struct CountsResponse {
    pub total: i64,
    pub group_by: Dimension,
    pub window: Window,
    pub buckets: Vec<Count>,
}
#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct Point {
    pub bucket_start: DateTime<Utc>,
    pub count: i64,
}
#[derive(Serialize, ToSchema)]
pub struct TimeseriesResponse {
    pub total: i64,
    pub bucket: Bucket,
    pub window: Window,
    pub filters: Filters,
    pub points: Vec<Point>,
}
#[derive(Serialize, ToSchema)]
pub struct NumericSummary {
    pub field: String,
    pub samples: i64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>,
}
#[derive(Serialize, ToSchema)]
pub struct SummaryResponse {
    pub total_events: i64,
    pub unique_sources: i64,
    pub unique_types: i64,
    pub first_occurred_at: Option<DateTime<Utc>>,
    pub last_occurred_at: Option<DateTime<Utc>>,
    pub by_type: Vec<Count>,
    pub by_source: Vec<Count>,
    pub numeric_summary: NumericSummary,
    pub window: Window,
    pub filters: Filters,
}

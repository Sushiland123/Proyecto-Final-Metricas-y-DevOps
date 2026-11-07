//! Repository port: the application does not depend on SQLx or PostgreSQL details.
use crate::{domain::*, error::Error};
use async_trait::async_trait;
use uuid::Uuid;
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Confirm basic dependency connectivity without applying schema changes.
    async fn ready(&self) -> Result<(), Error>;
    /// All events commit together, or none do. Writes are never blindly retried.
    async fn create(&self, events: &[EventCreate]) -> Result<Vec<Event>, Error>;
    /// Read a UUID without conflating a missing event with an unavailable dependency.
    async fn get(&self, id: Uuid) -> Result<Option<Event>, Error>;
    /// Read total and page from a coherent snapshot, ordered by occurrence time and UUID.
    async fn list(&self, query: &ListQuery) -> Result<(i64, Vec<Event>), Error>;
    /// Count events in the half-open interval using an allow-listed grouping dimension.
    async fn counts(&self, query: &CountsQuery) -> Result<Vec<Count>, Error>;
    /// Return nonempty time buckets ordered in UTC.
    async fn timeseries(&self, query: &TimeseriesQuery) -> Result<Vec<Point>, Error>;
    /// Return coherent event totals, dimensions and representable numeric statistics.
    async fn summary(&self, query: &SummaryQuery) -> Result<SummaryResponse, Error>;
}

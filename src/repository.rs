//! Repository port: the application does not depend on SQLx or PostgreSQL details.
use crate::{domain::*, error::Error};
use async_trait::async_trait;
use uuid::Uuid;
#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn ready(&self) -> Result<(), Error>;
    /// All events commit together, or none do. Writes are never blindly retried.
    async fn create(&self, events: &[EventCreate]) -> Result<Vec<Event>, Error>;
    async fn get(&self, id: Uuid) -> Result<Option<Event>, Error>;
    async fn list(&self, query: &ListQuery) -> Result<(i64, Vec<Event>), Error>;
    async fn counts(&self, query: &CountsQuery) -> Result<Vec<Count>, Error>;
    async fn timeseries(&self, query: &TimeseriesQuery) -> Result<Vec<Point>, Error>;
    async fn summary(&self, query: &SummaryQuery) -> Result<SummaryResponse, Error>;
}

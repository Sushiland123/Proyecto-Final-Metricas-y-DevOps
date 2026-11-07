//! Application use cases. Authorization and validation precede repository access.
use crate::{
    auth::{Principal, Role},
    domain::*,
    error::Error,
    repository::EventRepository,
};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;
#[derive(Clone)]
pub struct EventService {
    repository: Arc<dyn EventRepository>,
}
impl EventService {
    pub fn new(repository: Arc<dyn EventRepository>) -> Self {
        Self { repository }
    }
    pub async fn ready(&self) -> Result<(), Error> {
        self.repository.ready().await
    }
    pub async fn create(
        &self,
        principal: &Principal,
        batch: EventBatchCreate,
    ) -> Result<BatchResponse, Error> {
        principal.require(Role::Producer)?;
        batch.validate()?;
        for event in &batch.events {
            principal.require_source(&event.source)?;
        }
        let mut seen = HashSet::new();
        for event in &batch.events {
            if !seen.insert((&event.source, &event.external_id)) {
                return Err(Error::Duplicate);
            }
        }
        let events = self.repository.create(&batch.events).await?;
        Ok(BatchResponse {
            inserted: events.len(),
            events,
        })
    }
    pub async fn get(&self, principal: &Principal, id: Uuid) -> Result<Event, Error> {
        principal.require(Role::Analyst)?;
        self.repository.get(id).await?.ok_or(Error::NotFound)
    }
    pub async fn list(
        &self,
        principal: &Principal,
        query: ListQuery,
    ) -> Result<ListResponse, Error> {
        principal.require(Role::Analyst)?;
        query.validate()?;
        let (total, items) = self.repository.list(&query).await?;
        Ok(ListResponse {
            total,
            limit: query.limit,
            offset: query.offset,
            window: query.window(),
            items,
        })
    }
    pub async fn counts(
        &self,
        principal: &Principal,
        query: CountsQuery,
    ) -> Result<CountsResponse, Error> {
        principal.require(Role::Analyst)?;
        query.validate()?;
        let mut buckets = self.repository.counts(&query).await?;
        // The wire contract must not depend on the database locale/collation.
        buckets.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(CountsResponse {
            total: buckets.iter().map(|b| b.count).sum(),
            group_by: query.group_by,
            window: query.window(),
            buckets,
        })
    }
    pub async fn timeseries(
        &self,
        principal: &Principal,
        query: TimeseriesQuery,
    ) -> Result<TimeseriesResponse, Error> {
        principal.require(Role::Analyst)?;
        query.validate()?;
        let points = self.repository.timeseries(&query).await?;
        Ok(TimeseriesResponse {
            total: points.iter().map(|p| p.count).sum(),
            bucket: query.bucket,
            window: query.window(),
            filters: query.filters(),
            points,
        })
    }
    pub async fn summary(
        &self,
        principal: &Principal,
        query: SummaryQuery,
    ) -> Result<SummaryResponse, Error> {
        principal.require(Role::Analyst)?;
        query.validate()?;
        self.repository.summary(&query).await
    }
}

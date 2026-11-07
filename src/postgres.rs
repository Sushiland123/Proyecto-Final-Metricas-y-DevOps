//! SQLx adapter. Bounded async pool, transactional writes and retryable reads only.
use crate::{config::Settings, domain::*, error::Error, repository::EventRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{
    PgPool, Postgres, QueryBuilder, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{future::Future, str::FromStr, time::Duration};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresRepository {
    pool: Option<PgPool>,
    retries: u32,
    backoff: Duration,
}

impl PostgresRepository {
    /// Lazy connections preserve liveness while PostgreSQL is unavailable.
    pub fn new(settings: &Settings) -> Self {
        let pool = settings.database_url.as_deref()
            .and_then(|url| PgConnectOptions::from_str(url).ok())
            .map(|options| {
                let timeout = settings.statement_timeout_ms.to_string();
                PgPoolOptions::new().max_connections(10).min_connections(0)
                    .acquire_timeout(settings.connect_timeout)
                    .test_before_acquire(true)
                    .after_connect(move |connection, _| {
                        let timeout = timeout.clone();
                        Box::pin(async move {
                            sqlx::query("SELECT set_config('statement_timeout', $1, false), set_config('TimeZone', 'UTC', false)")
                                .bind(timeout).execute(connection).await?;
                            Ok(())
                        })
                    }).connect_lazy_with(options)
            });
        Self {
            pool,
            retries: settings.read_max_retries,
            backoff: settings.retry_backoff,
        }
    }

    pub fn pool(&self) -> Result<&PgPool, Error> {
        self.pool.as_ref().ok_or(Error::Unavailable)
    }

    async fn read<T, F, Fut>(&self, operation: F) -> Result<T, Error>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, sqlx::Error>>,
    {
        for attempt in 0..=self.retries {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(error) if retryable(&error) && attempt < self.retries => {
                    tokio::time::sleep(self.backoff * (1 << attempt)).await;
                }
                Err(_) => return Err(Error::Unavailable),
            }
        }
        Err(Error::Unavailable)
    }
}

fn retryable(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::PoolTimedOut
        | sqlx::Error::WorkerCrashed => true,
        sqlx::Error::Database(db) => db.code().is_some_and(|code| {
            code.starts_with("08")
                || matches!(
                    code.as_ref(),
                    "57P01" | "57P02" | "57P03" | "40001" | "40P01"
                )
        }),
        _ => false,
    }
}

fn write_error(error: sqlx::Error) -> Error {
    if let sqlx::Error::Database(db) = &error
        && db.code().as_deref() == Some("23505")
        && db.constraint() == Some("uq_events_source_external_id")
    {
        return Error::Duplicate;
    }
    Error::Unavailable
}

fn filtered<'a>(sql: &str, window: &Window, filters: &'a Filters) -> QueryBuilder<'a, Postgres> {
    let mut query = QueryBuilder::new(sql.to_owned());
    query
        .push(" WHERE occurred_at >= ")
        .push_bind(window.from)
        .push(" AND occurred_at < ")
        .push_bind(window.to);
    if let Some(source) = &filters.source {
        query.push(" AND source = ").push_bind(source);
    }
    if let Some(event_type) = &filters.event_type {
        query.push(" AND type = ").push_bind(event_type);
    }
    query
}

async fn snapshot(pool: &PgPool) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[derive(sqlx::FromRow)]
struct Totals {
    total_events: i64,
    unique_sources: i64,
    unique_types: i64,
    first_occurred_at: Option<DateTime<Utc>>,
    last_occurred_at: Option<DateTime<Utc>>,
    numeric_samples: i64,
    numeric_min: Option<f64>,
    numeric_max: Option<f64>,
    numeric_avg: Option<f64>,
}

#[async_trait]
impl EventRepository for PostgresRepository {
    async fn ready(&self) -> Result<(), Error> {
        sqlx::query("SELECT 1")
            .execute(self.pool()?)
            .await
            .map_err(|_| Error::Unavailable)?;
        Ok(())
    }

    async fn create(&self, events: &[EventCreate]) -> Result<Vec<Event>, Error> {
        let mut tx = self.pool()?.begin().await.map_err(write_error)?;
        let now = Utc::now();
        if events.is_empty() {
            return Err(Error::InvalidInput);
        }
        let ids: Vec<_> = events.iter().map(|_| Uuid::new_v4()).collect();
        let mut insert = QueryBuilder::<Postgres>::new(
            "INSERT INTO events (id, source, external_id, type, occurred_at, ingested_at, payload) ",
        );
        insert.push_values(events.iter().zip(&ids), |mut row, (event, id)| {
            row.push_bind(*id)
                .push_bind(&event.source)
                .push_bind(&event.external_id)
                .push_bind(&event.event_type)
                .push_bind(event.occurred_at)
                .push_bind(now)
                .push_bind(sqlx::types::Json(&event.payload));
        });
        let stored = insert
            .push(" RETURNING *")
            .build_query_as::<Event>()
            .fetch_all(&mut *tx)
            .await;
        let mut result = match stored {
            Ok(stored) => stored,
            Err(error) => {
                let public = write_error(error);
                tx.rollback().await.map_err(|_| Error::Unavailable)?;
                return Err(public);
            }
        };
        // RETURNING has no ordering guarantee. Restore the explicit input order.
        let positions: std::collections::HashMap<_, _> =
            ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        result.sort_by_key(|event| positions.get(&event.id).copied().unwrap_or(usize::MAX));
        tx.commit().await.map_err(write_error)?;
        Ok(result)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Event>, Error> {
        let pool = self.pool()?;
        self.read(|| {
            sqlx::query_as("SELECT * FROM events WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
        })
        .await
    }

    async fn list(&self, query: &ListQuery) -> Result<(i64, Vec<Event>), Error> {
        let pool = self.pool()?;
        self.read(|| async {
            let mut tx = snapshot(pool).await?;
            let filters = query.filters();
            let total: i64 = filtered("SELECT count(*) FROM events", &query.window(), &filters)
                .build_query_scalar()
                .fetch_one(&mut *tx)
                .await?;
            let events = filtered("SELECT * FROM events", &query.window(), &filters)
                .push(" ORDER BY occurred_at, id LIMIT ")
                .push_bind(query.limit)
                .push(" OFFSET ")
                .push_bind(query.offset)
                .build_query_as()
                .fetch_all(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok((total, events))
        })
        .await
    }

    async fn counts(&self, query: &CountsQuery) -> Result<Vec<Count>, Error> {
        let pool = self.pool()?;
        let column = query.group_by.column();
        let sql = format!(
            "SELECT {column} AS key, count(*) AS count FROM events WHERE occurred_at >= $1 AND occurred_at < $2 GROUP BY {column} ORDER BY {column}"
        );
        self.read(|| {
            sqlx::query_as(&sql)
                .bind(query.from)
                .bind(query.to)
                .fetch_all(pool)
        })
        .await
    }

    async fn timeseries(&self, query: &TimeseriesQuery) -> Result<Vec<Point>, Error> {
        let pool = self.pool()?;
        self.read(|| async {
            let filters = query.filters();
            let mut sql = QueryBuilder::<Postgres>::new("SELECT date_trunc(");
            sql.push_bind(query.bucket.name()).push(", occurred_at, 'UTC') AS bucket_start, count(*) AS count FROM events WHERE occurred_at >= ")
                .push_bind(query.from).push(" AND occurred_at < ").push_bind(query.to);
            if let Some(source) = &filters.source { sql.push(" AND source = ").push_bind(source); }
            if let Some(kind) = &filters.event_type { sql.push(" AND type = ").push_bind(kind); }
            sql.push(" GROUP BY bucket_start ORDER BY bucket_start").build_query_as().fetch_all(pool).await
        }).await
    }

    async fn summary(&self, query: &SummaryQuery) -> Result<SummaryResponse, Error> {
        let pool = self.pool()?;
        self.read(|| async {
            let mut tx = snapshot(pool).await?;
            let filters = query.filters();
            let mut sql = QueryBuilder::<Postgres>::new("WITH selected AS (SELECT *, CASE WHEN jsonb_typeof(payload -> ");
            sql.push_bind(&query.numeric_field).push(") = 'number' THEN (payload ->> ")
                .push_bind(&query.numeric_field).push(")::numeric ELSE NULL END AS numeric_value FROM events WHERE occurred_at >= ")
                .push_bind(query.from).push(" AND occurred_at < ").push_bind(query.to);
            if let Some(source) = &filters.source { sql.push(" AND source = ").push_bind(source); }
            if let Some(kind) = &filters.event_type { sql.push(" AND type = ").push_bind(kind); }
            sql.push(") SELECT count(*) AS total_events, count(DISTINCT source) AS unique_sources, count(DISTINCT type) AS unique_types,
                min(occurred_at) AS first_occurred_at, max(occurred_at) AS last_occurred_at,
                count(numeric_value) AS numeric_samples, min(numeric_value)::float8 AS numeric_min,
                max(numeric_value)::float8 AS numeric_max, avg(numeric_value)::float8 AS numeric_avg FROM selected");
            let totals: Totals = sql.build_query_as().fetch_one(&mut *tx).await?;
            let by_type = filtered("SELECT type AS key, count(*) AS count FROM events", &query.window(), &filters)
                .push(" GROUP BY type ORDER BY type").build_query_as().fetch_all(&mut *tx).await?;
            let by_source = filtered("SELECT source AS key, count(*) AS count FROM events", &query.window(), &filters)
                .push(" GROUP BY source ORDER BY source").build_query_as().fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok(SummaryResponse {
                total_events: totals.total_events, unique_sources: totals.unique_sources, unique_types: totals.unique_types,
                first_occurred_at: totals.first_occurred_at, last_occurred_at: totals.last_occurred_at,
                by_type, by_source, numeric_summary: NumericSummary {
                    field: query.numeric_field.clone(), samples: totals.numeric_samples,
                    min: totals.numeric_min, max: totals.numeric_max, avg: totals.numeric_avg,
                }, window: query.window(), filters,
            })
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn repository() -> PostgresRepository {
        PostgresRepository {
            pool: None,
            retries: 2,
            backoff: Duration::ZERO,
        }
    }

    #[tokio::test]
    async fn transient_reads_retry_then_recover_within_bound() {
        let attempts = AtomicUsize::new(0);
        let result = repository()
            .read(|| async {
                if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(sqlx::Error::Io(std::io::Error::from(
                        std::io::ErrorKind::ConnectionReset,
                    )))
                } else {
                    Ok(42)
                }
            })
            .await;
        assert_eq!(result, Ok(42));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhausted_reads_return_safe_dependency_error() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), Error> = repository()
            .read(|| async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(sqlx::Error::PoolTimedOut)
            })
            .await;
        assert_eq!(result, Err(Error::Unavailable));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn nontransient_sql_errors_are_not_retried() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), Error> = repository()
            .read(|| async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(sqlx::Error::Protocol("secret internal detail".into()))
            })
            .await;
        assert_eq!(result, Err(Error::Unavailable));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}

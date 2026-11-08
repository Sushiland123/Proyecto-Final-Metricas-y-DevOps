#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Run explicitly: cargo test --test postgres -- --ignored
#[path = "scenario3/mod.rs"]
mod scenario3;
mod support;
use event_service::{MIGRATOR, domain::*, error::Error, repository::EventRepository};
use serde_json::{Value, json};
use support::*;

/// Build one typed event with the shared wire fixture for direct repository tests.
fn input(source: &str, id: &str) -> EventCreate {
    serde_json::from_value(event(source, id)).unwrap()
}
/// Restrict HTTP queries to the source uniquely allocated by the current test.
fn window(source: &str) -> String {
    format!("from=2040-01-01T12:00:00Z&to=2040-01-01T13:00:00Z&source={source}")
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Verify round-trip data, numeric precision, duplicate immutability and the complete listing envelope.
async fn ingestion_get_duplicate_and_pagination() {
    let db = Database::new().await;
    let app = db.app();
    let large_integer = "1234567890123456789012345678901234567890";
    let mut first = event(&db.source, "first");
    first["payload"]["large_integer"] = serde_json::from_str(large_integer).unwrap();
    assert_eq!(
        request(&app, "GET", "/health/ready", None, None).await.0,
        200
    );
    let original = request(
        &app,
        "POST",
        "/v1/events",
        Some("test-producer"),
        Some(first),
    )
    .await;
    assert_eq!(original.0, 201);
    assert_eq!(
        original.1["payload"]["large_integer"].to_string(),
        large_integer
    );
    assert_eq!(original.1.as_object().unwrap().len(), 7);
    let id = original.1["id"].as_str().unwrap();
    uuid::Uuid::parse_str(id).unwrap();
    let restored = request(
        &app,
        "GET",
        &format!("/v1/events/{id}"),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(restored.1, original.1);
    let mut duplicate = event(&db.source, "first");
    duplicate["payload"] = json!({"value":999});
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events",
            Some("test-producer"),
            Some(duplicate),
        )
        .await,
        409,
        "DUPLICATE",
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/v1/events/{id}"),
            Some("test-analyst"),
            None
        )
        .await
        .1,
        original.1
    );
    assert_error(
        &request(
            &app,
            "GET",
            &format!("/v1/events/{}", uuid::Uuid::new_v4()),
            Some("test-analyst"),
            None,
        )
        .await,
        404,
        "NOT_FOUND",
    );
    for i in 1..4 {
        let mut value = event(&db.source, &format!("page-{i}"));
        value["occurred_at"] = json!(format!("2040-01-01T12:0{i}:00Z"));
        value["type"] = json!(if i % 2 == 0 { "temperature" } else { "status" });
        assert_eq!(
            request(
                &app,
                "POST",
                "/v1/events",
                Some("test-producer"),
                Some(value)
            )
            .await
            .0,
            201
        );
    }
    let page = request(
        &app,
        "GET",
        &format!("/v1/events?{}&limit=2&offset=1", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(page.0, 200);
    assert_eq!(page.1["total"], 4);
    assert_eq!(page.1["filters"], json!({"source":db.source,"type":null}));
    assert_eq!(page.1["items"][0]["external_id"], "page-1");
    assert_eq!(page.1["items"][1]["external_id"], "page-2");
    let filtered = request(
        &app,
        "GET",
        &format!("/v1/events?{}&type=status", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(filtered.1["total"], 2);
    assert_eq!(
        filtered.1["filters"],
        json!({"source":db.source,"type":"status"})
    );
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Ensure storage collisions roll back the full batch and all source permissions precede insertion.
async fn batch_is_atomic_and_source_scope_is_enforced() {
    let db = Database::new().await;
    let app = db.app();
    let batch = json!({"events": [event(&db.source, "a"), event(&db.source, "b")]});
    let created = request(
        &app,
        "POST",
        "/v1/events/batch",
        Some("test-producer"),
        Some(batch),
    )
    .await;
    assert_eq!(created.0, 201);
    assert_eq!(created.1["inserted"], 2);
    assert_eq!(created.1["events"][0]["external_id"], "a");
    let collision = json!({"events": [event(&db.source, "must-rollback"), event(&db.source, "a")]});
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events/batch",
            Some("test-producer"),
            Some(collision),
        )
        .await,
        409,
        "DUPLICATE",
    );
    let scoped =
        json!({"events": [event("sensor-a", "never-write"), event(&db.source, "scope-denied")]});
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events/batch",
            Some("scoped-a"),
            Some(scoped),
        )
        .await,
        403,
        "FORBIDDEN",
    );
    let listed = request(
        &app,
        "GET",
        &format!("/v1/events?{}", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(listed.1["total"], 2);
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Race two transactions for one natural key and verify only the confirmed winner is readable.
async fn concurrent_duplicate_has_exactly_one_winner() {
    let db = Database::new().await;
    let repository = db.repository();
    let events = [input(&db.source, "concurrent")];
    let (a, b) = tokio::join!(repository.create(&events), repository.create(&events));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(
        (&a, &b),
        (Err(Error::Duplicate), Ok(_)) | (Ok(_), Err(Error::Duplicate))
    ));
    let winner = a.or(b).unwrap();
    let restored = repository.get(winner[0].id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(&winner[0]).unwrap()
    );
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Verify equivalent offsets, inclusive start/exclusive end, both dimensions and empty intervals.
async fn counts_use_half_open_utc_windows_and_sorted_buckets() {
    let db = Database::new().await;
    let repository = db.repository();
    let mut events = Vec::new();
    let kind = format!("metric-{}", uuid::Uuid::new_v4().simple());
    for (index, date) in [
        "2040-01-01T11:59:59.999999Z",
        "2040-01-01T14:00:00+02:00",
        "2040-01-01T12:59:59.999999Z",
        "2040-01-01T13:00:00Z",
    ]
    .iter()
    .enumerate()
    {
        let mut value = input(&db.source, &index.to_string());
        value.occurred_at = date.parse().unwrap();
        value.event_type = kind.clone();
        events.push(value);
    }
    repository.create(&events).await.unwrap();
    let query: CountsQuery = serde_json::from_value(json!({"from":"2040-01-01T14:00:00+02:00", "to":"2040-01-01T15:00:00+02:00", "group_by":"source"})).unwrap();
    let counts = repository.counts(&query).await.unwrap();
    assert_eq!(counts.iter().find(|c| c.key == db.source).unwrap().count, 2);
    assert!(counts.windows(2).all(|pair| pair[0].key <= pair[1].key));
    let types = repository
        .counts(&CountsQuery {
            group_by: Dimension::Type,
            ..query
        })
        .await
        .unwrap();
    assert_eq!(types.iter().find(|c| c.key == kind).unwrap().count, 2);
    let empty: CountsQuery = serde_json::from_value(
        json!({"from":"9000-01-01T00:00:00Z", "to":"9000-01-02T00:00:00Z", "group_by":"type"}),
    )
    .unwrap();
    assert!(repository.counts(&empty).await.unwrap().is_empty());
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Validate UTC resolutions, numeric-only samples and null/empty aggregate contracts against PostgreSQL.
async fn timeseries_summary_and_empty_results() {
    let db = Database::new().await;
    let app = db.app();
    for (i, payload) in [
        json!({"value":15}),
        json!({"value":25}),
        json!({"value":35}),
        json!({"value":"99"}),
        json!({"value":null}),
        json!({"value":true}),
        json!({"other":10}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut value = event(&db.source, &i.to_string());
        value["payload"] = payload;
        assert_eq!(
            request(
                &app,
                "POST",
                "/v1/events",
                Some("test-producer"),
                Some(value)
            )
            .await
            .0,
            201
        );
    }
    let summary = request(
        &app,
        "GET",
        &format!("/v1/analysis/summary?{}", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(summary.0, 200);
    assert_eq!(summary.1["total_events"], 7);
    assert_eq!(summary.1["unique_sources"], 1);
    assert_eq!(summary.1["unique_types"], 1);
    assert_eq!(
        summary.1["numeric_summary"],
        json!({"field":"value","samples":3,"min":15.0,"max":35.0,"avg":25.0})
    );
    for bucket in ["minute", "hour", "day"] {
        let series = request(
            &app,
            "GET",
            &format!(
                "/v1/analysis/timeseries?{}&bucket={bucket}",
                window(&db.source)
            ),
            Some("test-analyst"),
            None,
        )
        .await;
        assert_eq!(series.0, 200);
        assert_eq!(series.1["total"], 7);
        assert_eq!(series.1["points"][0]["count"], 7);
    }
    let empty = request(
        &app,
        "GET",
        &format!(
            "/v1/analysis/summary?{}&type=nonexistent",
            window(&db.source)
        ),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(empty.0, 200);
    assert_eq!(empty.1["total_events"], 0);
    assert_eq!(empty.1["numeric_summary"]["samples"], 0);
    assert!(empty.1["numeric_summary"]["avg"].is_null());
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Prevent oversized JSON numerics from denying analyst reads while preserving their exact stored data.
/// Also exercise valid values above 1e308 and underflow/cancellation during float64 conversion.
async fn summary_handles_extreme_numeric_payloads_without_losing_events() {
    let db = Database::new().await;
    let app = db.app();
    for (index, number) in ["1e400", "-1e400", "1e-400", "1.5e308", "-1.5e308", "0"]
        .into_iter()
        .enumerate()
    {
        let mut value = event(&db.source, &format!("extreme-{index}"));
        value["payload"]["value"] = serde_json::from_str(number).unwrap();
        let response = request(
            &app,
            "POST",
            "/v1/events",
            Some("test-producer"),
            Some(value),
        )
        .await;
        assert_eq!(response.0, 201);
    }
    let summary = request(
        &app,
        "GET",
        &format!("/v1/analysis/summary?{}", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(summary.0, 200);
    assert_eq!(summary.1["total_events"], 6);
    assert_eq!(summary.1["numeric_summary"]["samples"], 4);
    assert_eq!(summary.1["numeric_summary"]["min"].as_f64(), Some(-1.5e308));
    assert_eq!(summary.1["numeric_summary"]["max"].as_f64(), Some(1.5e308));
    assert_eq!(summary.1["numeric_summary"]["avg"].as_f64(), Some(0.0));
    // Unrepresentable statistical samples are still available as exact event data.
    let listed = request(
        &app,
        "GET",
        &format!("/v1/events?{}", window(&db.source)),
        Some("test-analyst"),
        None,
    )
    .await;
    assert_eq!(listed.1["total"], 6);
    let huge = listed.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["external_id"] == "extreme-0")
        .unwrap();
    assert_eq!(
        huge["payload"]["value"].to_string(),
        format!("1{}", "0".repeat(400))
    );
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Reopen persistence with a fresh pool, reapply migrations and verify data plus required indexes.
async fn data_survives_new_pool_and_repeated_migrations() {
    let db = Database::new().await;
    let repository = db.repository();
    let original = repository
        .create(&[input(&db.source, "durable")])
        .await
        .unwrap()
        .remove(0);
    repository.pool().unwrap().close().await;
    let replacement = db.repository();
    MIGRATOR.run(replacement.pool().unwrap()).await.unwrap();
    MIGRATOR.run(replacement.pool().unwrap()).await.unwrap();
    let restored = replacement.get(original.id).await.unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&original).unwrap(),
        serde_json::to_value(restored).unwrap()
    );
    let migrations: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(migrations as usize, MIGRATOR.iter().count());
    let indexes: Vec<String> =
        sqlx::query_scalar("SELECT indexname FROM pg_indexes WHERE tablename = 'events'")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for name in [
        "ix_events_occurred_at_type",
        "ix_events_occurred_at_source",
        "uq_events_source_external_id",
    ] {
        assert!(indexes.iter().any(|s| s == name));
    }
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Confirm deduplication uses the full source/external-ID pair rather than a global external ID.
async fn external_id_can_repeat_in_different_source() {
    let db = Database::new().await;
    let second = Database::new().await;
    let repo = db.repository();
    let events = repo
        .create(&[input(&db.source, "shared"), input(&second.source, "shared")])
        .await
        .unwrap();
    assert_ne!(events[0].id, events[1].id);
    db.cleanup().await;
    second.cleanup().await;
}

// A private schema isolates migration adoption tests without dropping any shared
// objects. The schema name is generated by this test, never supplied by a client.
/// Create an experiment-owned schema and pool so migration tests cannot alter shared application tables.
async fn isolated_schema(db: &Database) -> (String, sqlx::PgPool) {
    let name = format!("migration_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {name}"))
        .execute(&db.pool)
        .await
        .unwrap();
    let schema = name.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let schema = schema.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {schema}"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(db.settings.database_url.as_deref().unwrap())
        .await
        .unwrap();
    (name, pool)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Adopt an existing compatible schema without changing events or historical migration metadata.
async fn migration_adopts_legacy_schema_without_rewriting_data() {
    let db = Database::new().await;
    let (schema, pool) = isolated_schema(&db).await;
    sqlx::raw_sql("CREATE TABLE events (
        id uuid PRIMARY KEY, source varchar(64) NOT NULL, external_id varchar(128) NOT NULL,
        type varchar(64) NOT NULL, occurred_at timestamptz NOT NULL, ingested_at timestamptz NOT NULL,
        payload jsonb NOT NULL, CONSTRAINT uq_events_source_external_id UNIQUE(source,external_id));
        CREATE TABLE alembic_version(version_num varchar(32) PRIMARY KEY);
        INSERT INTO alembic_version VALUES ('20261102_02');
        INSERT INTO events VALUES ('00000000-0000-0000-0000-000000000001','legacy','id','metric','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','{\"value\":23.5}');")
        .execute(&pool).await.unwrap();
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(events) FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(events) FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let legacy: String = sqlx::query_scalar("SELECT version_num FROM alembic_version")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(legacy, "20261102_02");
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&db.pool)
        .await
        .unwrap();
    db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing to dedicated PostgreSQL"]
/// Abort adoption of an incompatible table and verify its existing row remains untouched.
async fn migration_rejects_incompatible_existing_table() {
    let db = Database::new().await;
    let (schema, pool) = isolated_schema(&db).await;
    sqlx::raw_sql("CREATE TABLE events (id integer PRIMARY KEY); INSERT INTO events VALUES (42);")
        .execute(&pool)
        .await
        .unwrap();
    assert!(MIGRATOR.run(&pool).await.is_err());
    let id: i32 = sqlx::query_scalar("SELECT id FROM events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(id, 42);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&db.pool)
        .await
        .unwrap();
    db.cleanup().await;
}

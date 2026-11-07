#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use event_service::domain::*;
use serde_json::{Value, json};

fn validate(value: Value) -> bool {
    serde_json::from_value::<EventCreate>(value).is_ok_and(|event| event.validate().is_ok())
}

#[test]
fn event_normalizes_utc_and_preserves_json() {
    let mut value = support::event("sensor-a", "id");
    value["occurred_at"] = json!("2026-10-21T12:00:00+02:00");
    let event: EventCreate = serde_json::from_value(value).unwrap();
    event.validate().unwrap();
    assert_eq!(event.occurred_at.to_rfc3339(), "2026-10-21T10:00:00+00:00");
    assert_eq!(event.payload["value"], 23.5);
    let large_integer = "1234567890123456789012345678901234567890";
    let value: serde_json::Value = serde_json::from_str(large_integer).unwrap();
    assert_eq!(
        value.to_string(),
        large_integer,
        "JSON integers must not silently lose precision"
    );
}

#[test]
fn rejects_invalid_identifiers_dates_payloads_and_extra_fields() {
    for field in ["source", "type"] {
        for invalid in ["", "bad source", "../x", "é", "a\n", &"x".repeat(65)] {
            let mut value = support::event("sensor-a", "id");
            value[field] = json!(invalid);
            assert!(!validate(value), "accepted {field}={invalid:?}");
        }
    }
    for (field, invalid) in [
        ("external_id", json!("")),
        ("external_id", json!("e".repeat(129))),
        ("occurred_at", json!("2026-10-21T12:00:00")),
        ("occurred_at", json!(1792584000)),
        ("payload", json!([])),
        ("unexpected", json!("secret")),
    ] {
        let mut value = support::event("sensor-a", "id");
        value[field] = invalid;
        assert!(!validate(value));
    }
}

#[test]
fn payload_limit_counts_utf8_bytes() {
    let mut value = support::event("sensor-a", "id");
    value["payload"] = json!({"data": "x".repeat(16 * 1024 - 11)});
    assert!(validate(value.clone()));
    value["payload"]["data"] = json!("x".repeat(16 * 1024 - 10));
    assert!(!validate(value.clone()));
    value["payload"]["data"] = json!("é".repeat(8192));
    assert!(!validate(value));
}

#[test]
fn batch_bounds_and_unknown_fields() {
    for count in [0, 1, 100, 101] {
        let batch: EventBatchCreate = serde_json::from_value(
            json!({"events": vec![support::event("sensor-a", "id"); count]}),
        )
        .unwrap();
        assert_eq!(batch.validate().is_ok(), (1..=100).contains(&count));
    }
    assert!(serde_json::from_value::<EventBatchCreate>(json!({"events": [], "extra": 1})).is_err());
}

#[test]
fn validates_queries_and_window_limits() {
    let window = json!({"from": "2026-01-01T00:00:00Z", "to": "2026-01-02T00:00:00Z"});
    let mut counts = window.clone();
    counts["group_by"] = json!("type");
    assert!(
        serde_json::from_value::<CountsQuery>(counts.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    counts["to"] = counts["from"].clone();
    assert!(
        serde_json::from_value::<CountsQuery>(counts)
            .unwrap()
            .validate()
            .is_err()
    );
    for (field, value) in [
        ("limit", json!(0)),
        ("limit", json!(101)),
        ("offset", json!(-1)),
        ("offset", json!(100001)),
    ] {
        let mut query = window.clone();
        query[field] = value;
        assert!(
            serde_json::from_value::<ListQuery>(query)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for field in ["limit", "offset"] {
        let mut query = window.clone();
        query[field] = json!(true);
        assert!(serde_json::from_value::<ListQuery>(query).is_err());
    }
    for (bucket, days) in [("minute", 7), ("hour", 90), ("day", 366)] {
        let from: chrono::DateTime<chrono::Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let end = from + chrono::Duration::days(days);
        let query = json!({"from": from, "to": end, "bucket": bucket});
        assert!(
            serde_json::from_value::<TimeseriesQuery>(query.clone())
                .unwrap()
                .validate()
                .is_ok()
        );
        let mut query = query;
        query["to"] = json!(end + chrono::Duration::seconds(1));
        assert!(
            serde_json::from_value::<TimeseriesQuery>(query)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut summary = window;
    summary["numeric_field"] = json!("value; DROP TABLE events");
    assert!(
        serde_json::from_value::<SummaryQuery>(summary)
            .unwrap()
            .validate()
            .is_err()
    );
}

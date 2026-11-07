#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use event_service::{
    auth::{Role, authenticate},
    config::Settings,
    error::Error,
};
use std::collections::BTreeMap;

#[test]
/// Verify vertical role separation, lateral source scoping and unknown-key rejection.
fn roles_and_source_scope_cannot_escalate() {
    let settings = support::settings();
    let producer = authenticate(Some("test-producer"), &settings).unwrap();
    assert!(producer.require(Role::Producer).is_ok());
    assert_eq!(producer.require(Role::Analyst), Err(Error::Forbidden));
    assert!(producer.require_source("any-source").is_ok());
    let analyst = authenticate(Some("test-analyst"), &settings).unwrap();
    assert!(analyst.require(Role::Analyst).is_ok());
    assert_eq!(analyst.require_source("sensor-a"), Err(Error::Forbidden));
    let scoped = authenticate(Some("scoped-a"), &settings).unwrap();
    assert!(scoped.require_source("sensor-a").is_ok());
    assert_eq!(scoped.require_source("sensor-b"), Err(Error::Forbidden));
    for key in [None, Some(""), Some("unknown")] {
        assert!(authenticate(key, &settings).is_err());
    }
}

#[test]
/// Reject malformed source grammar and ambiguous global/scoped credentials for all identities.
fn malformed_or_colliding_settings_fail_closed() {
    for raw in [
        "bad",
        "sensor-a:",
        ":key",
        "bad source:key",
        "sensor-a:a,sensor-a:b",
        "sensor-a:test-producer",
        "sensor-a:test-analyst",
        "sensor-a:a,",
    ] {
        let mut settings = support::settings();
        let values = BTreeMap::from([
            ("EVENT_PRODUCER_KEY".into(), "test-producer".into()),
            ("EVENT_ANALYST_KEY".into(), "test-analyst".into()),
            ("EVENT_PRODUCER_SOURCE_KEYS".into(), raw.into()),
        ]);
        let parsed = Settings::from_map(&values);
        settings.source_keys = parsed.source_keys;
        settings.auth_invalid = parsed.auth_invalid;
        assert!(
            authenticate(Some("test-producer"), &settings).is_err(),
            "accepted {raw}"
        );
    }
    for producer in [None, Some("".into()), Some("test-analyst".into())] {
        let mut settings = support::settings();
        settings.producer_key = producer;
        assert!(authenticate(Some("test-analyst"), &settings).is_err());
    }
}

#[test]
/// Preserve the supported many-sources-per-key authorization model without global privileges.
fn one_scoped_key_may_authorize_multiple_sources() {
    let mut settings = support::settings();
    settings
        .source_keys
        .insert("sensor-b".into(), "scoped-a".into());
    let principal = authenticate(Some("scoped-a"), &settings).unwrap();
    assert!(principal.require_source("sensor-a").is_ok());
    assert!(principal.require_source("sensor-b").is_ok());
}

#[test]
/// Check numeric bounds, invalid-value defaults and an explicitly disabled retry delay.
fn bounded_configuration_defaults_on_invalid_values() {
    let settings = Settings::from_map(&BTreeMap::from([
        ("DB_CONNECT_TIMEOUT_SECONDS".into(), "0".into()),
        ("DB_STATEMENT_TIMEOUT_MS".into(), "60001".into()),
        ("DB_READ_MAX_RETRIES".into(), "bad".into()),
        ("DB_READ_RETRY_BACKOFF_MS".into(), "0".into()),
    ]));
    assert_eq!(settings.connect_timeout.as_secs(), 3);
    assert_eq!(settings.statement_timeout_ms, 5000);
    assert_eq!(settings.read_max_retries, 1);
    assert!(settings.retry_backoff.is_zero());
}

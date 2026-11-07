//! Environment configuration. Secrets deliberately implement neither Debug nor Serialize.
use crate::domain::identifier;
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone)]
pub struct Settings {
    pub database_url: Option<String>,
    pub producer_key: Option<String>,
    pub analyst_key: Option<String>,
    pub source_keys: BTreeMap<String, String>,
    pub auth_invalid: bool,
    pub connect_timeout: Duration,
    pub statement_timeout_ms: u64,
    pub read_max_retries: u32,
    pub retry_backoff: Duration,
}
impl Settings {
    /// Snapshot process environment at startup; later changes require replacing the process.
    pub fn from_env() -> Self {
        Self::from_map(&std::env::vars().collect())
    }
    /// Parse an isolated environment, applying bounded numeric defaults and fail-closed key grammar.
    /// Missing storage configuration preserves liveness; invalid credentials never grant access.
    pub fn from_map(values: &BTreeMap<String, String>) -> Self {
        let present = |name: &str| values.get(name).filter(|s| !s.trim().is_empty()).cloned();
        let bounded = |name: &str, default, min, max| {
            values
                .get(name)
                .and_then(|s| s.trim().parse::<u64>().ok())
                .filter(|v| (min..=max).contains(v))
                .unwrap_or(default)
        };
        let mut source_keys = BTreeMap::new();
        let mut auth_invalid = false;
        if let Some(raw) = present("EVENT_PRODUCER_SOURCE_KEYS") {
            for entry in raw.split(',') {
                let entry = entry.trim();
                let split = entry.split_once(':').or_else(|| entry.split_once('='));
                match split {
                    Some((source, key)) if identifier(source.trim()) && !key.trim().is_empty() => {
                        if source_keys
                            .insert(source.trim().to_owned(), key.trim().to_owned())
                            .is_some()
                        {
                            auth_invalid = true;
                        }
                    }
                    _ => auth_invalid = true,
                }
            }
        }
        Self {
            database_url: present("DATABASE_URL"),
            producer_key: present("EVENT_PRODUCER_KEY"),
            analyst_key: present("EVENT_ANALYST_KEY"),
            source_keys,
            auth_invalid,
            connect_timeout: Duration::from_secs(bounded("DB_CONNECT_TIMEOUT_SECONDS", 3, 1, 60)),
            statement_timeout_ms: bounded("DB_STATEMENT_TIMEOUT_MS", 5000, 100, 60000),
            read_max_retries: bounded("DB_READ_MAX_RETRIES", 1, 0, 5) as u32,
            retry_backoff: Duration::from_millis(bounded("DB_READ_RETRY_BACKOFF_MS", 50, 0, 2000)),
        }
    }
}

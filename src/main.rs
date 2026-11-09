//! Composition root and operational CLI. Migrations are explicit, not run on startup.
use event_service::{
    MIGRATOR,
    api::{self, AppState},
    config::Settings,
    postgres::PostgresRepository,
    service::EventService,
};
use std::path::Path;
use std::time::Duration;
use std::{process::ExitCode, sync::Arc};
use utoipa::OpenApi;

/// Heartbeat file names inside `WORKER_HEALTH_DIR`.
const WORKER_LIVE_FILE: &str = "live";
const WORKER_READY_FILE: &str = "ready";
/// Must exceed the 60 s retry ceiling so a database outage never restarts the worker.
const WORKER_LIVE_MAX_AGE: Duration = Duration::from_secs(120);
/// The last successful poll must be this recent for the worker to count as ready.
const WORKER_READY_MAX_AGE: Duration = Duration::from_secs(30);

/// Record a heartbeat; a failed write surfaces as a failing probe, not as log noise.
fn touch_heartbeat(directory: Option<&Path>, name: &str) {
    if let Some(directory) = directory {
        let _ = std::fs::write(directory.join(name), b"ok");
    }
}

/// Report whether a heartbeat exists and was written within `max_age`.
fn heartbeat_is_fresh(path: &Path, max_age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age <= max_age)
}

/// Exec probe for the worker, which has no HTTP server and whose image has no shell.
fn worker_health(kind: Option<String>) -> Result<(), &'static str> {
    let (name, max_age) = match kind.as_deref() {
        Some("live") => (WORKER_LIVE_FILE, WORKER_LIVE_MAX_AGE),
        Some("ready") => (WORKER_READY_FILE, WORKER_READY_MAX_AGE),
        _ => return Err("Usage: event-service worker-health [live|ready]"),
    };
    let directory = std::env::var_os("WORKER_HEALTH_DIR").ok_or("WORKER_HEALTH_DIR required")?;
    if heartbeat_is_fresh(&Path::new(&directory).join(name), max_age) {
        Ok(())
    } else {
        Err("Worker heartbeat missing or stale")
    }
}

/// Bounded retry state for transient processor failures; success resets the delay.
struct RetryBackoff {
    initial: Duration,
    next: Duration,
}
impl RetryBackoff {
    /// Honor configured polling/read backoff with a 100 ms floor and 60 s ceiling.
    fn new(initial: Duration) -> Self {
        let initial = initial.clamp(Duration::from_millis(100), Duration::from_secs(60));
        Self {
            initial,
            next: initial,
        }
    }
    /// Return the current delay and double the next delay without overflowing.
    fn failure(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(Duration::from_secs(60));
        delay
    }
    /// Restore the configured first-retry delay after a successful batch or empty poll.
    fn reset(&mut self) {
        self.next = self.initial;
    }
}

#[tokio::main]
/// Initialize safe diagnostics and translate command failures to a nonzero process exit status.
async fn main() -> ExitCode {
    // The default hook prints panic payloads even when catch_unwind handles them.
    // Never send potentially sensitive panic messages to application logs.
    std::panic::set_hook(Box::new(|_| tracing::error!("unexpected panic")));
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "event_service=info,sqlx=off".into()),
        )
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!(message, "operation failed");
            ExitCode::FAILURE
        }
    }
}

/// Dispatch explicit serve/migrate/openapi operations without implicitly migrating during startup.
async fn run() -> Result<(), &'static str> {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "serve".to_owned());
    if command == "openapi" {
        println!(
            "{}",
            api::ApiDoc::openapi()
                .to_pretty_json()
                .map_err(|_| "Cannot serialize OpenAPI")?
        );
        return Ok(());
    }
    if command == "worker-health" {
        return worker_health(std::env::args().nth(2));
    }
    if !matches!(command.as_str(), "serve" | "migrate" | "process" | "drain") {
        return Err("Usage: event-service [serve|migrate|openapi|process|drain|worker-health]");
    }
    let settings = Settings::from_env();
    let repository = PostgresRepository::new(&settings);
    if matches!(command.as_str(), "process" | "drain") {
        let store = event_service::scenario::store::Store {
            pool: repository
                .pool()
                .map_err(|_| "DATABASE_URL required for processor")?
                .clone(),
        };
        let batch_size: i64 = std::env::var("PROCESSOR_BATCH_SIZE")
            .unwrap_or_else(|_| "100".into())
            .parse()
            .ok()
            .filter(|n| (1..=1000).contains(n))
            .ok_or("Invalid PROCESSOR_BATCH_SIZE")?;
        let delay: u64 = std::env::var("PROCESSOR_POLL_MS")
            .unwrap_or_else(|_| "100".into())
            .parse()
            .ok()
            .filter(|n| (1..=60000).contains(n))
            .ok_or("Invalid PROCESSOR_POLL_MS")?;
        let health_directory = std::env::var_os("WORKER_HEALTH_DIR");
        let health_directory = health_directory.as_deref().map(Path::new);
        let worker = async {
            let mut backoff =
                RetryBackoff::new(Duration::from_millis(delay).max(settings.retry_backoff));
            loop {
                touch_heartbeat(health_directory, WORKER_LIVE_FILE);
                let mut wait = Duration::from_millis(delay);
                match store.process(batch_size).await {
                    Ok(0) if command == "drain" => return Ok(()),
                    Ok(count) => {
                        touch_heartbeat(health_directory, WORKER_READY_FILE);
                        backoff.reset();
                        if count > 0 {
                            tracing::info!(count, "readings processed");
                        }
                    }
                    Err(error) => {
                        let code = error.details().1;
                        if command == "drain" {
                            tracing::error!(code, "drain failed");
                            return Err("Processor drain failed");
                        }
                        if error != event_service::error::Error::Unavailable {
                            tracing::error!(code, "non-retryable processor failure");
                            return Err("Processor configuration or operation invalid");
                        }
                        wait = backoff.failure();
                        tracing::warn!(
                            code,
                            retry_ms = wait.as_millis() as u64,
                            "processor dependency unavailable; retrying"
                        );
                    }
                }
                tokio::time::sleep(wait).await;
            }
        };
        return tokio::select! { result = worker => result, _ = shutdown() => Ok(()) };
    }
    if command == "migrate" {
        MIGRATOR
            .run(
                repository
                    .pool()
                    .map_err(|_| "Invalid or missing DATABASE_URL")?,
            )
            .await
            .map_err(|_| "Migration failed; check database schema and connectivity")?;
        tracing::info!("migrations applied");
        return Ok(());
    }
    let scenario = repository
        .pool()
        .ok()
        .cloned()
        .map(|pool| event_service::scenario::store::Store { pool });
    let state = AppState {
        settings: Arc::new(settings),
        service: EventService::new(Arc::new(repository)),
        scenario,
    };
    let address = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".to_owned());
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|_| "Cannot bind listener")?;
    tracing::info!(%address, "service listening");
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|_| "HTTP server failed")
}

/// Await SIGINT or Unix SIGTERM so Axum can finish active requests before process exit.
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// Transient failures exponentially back off at a fixed cap and success resets them.
    fn processor_retry_is_bounded_and_resets() {
        let mut retry = RetryBackoff::new(Duration::from_millis(250));
        assert_eq!(retry.failure(), Duration::from_millis(250));
        assert_eq!(retry.failure(), Duration::from_millis(500));
        for _ in 0..32 {
            retry.failure();
        }
        assert_eq!(retry.failure(), Duration::from_secs(60));
        retry.reset();
        assert_eq!(retry.failure(), Duration::from_millis(250));
    }
    #[test]
    /// Probes pass only on a recent heartbeat, and the file records the latest write.
    fn worker_heartbeat_requires_a_recent_write() {
        let directory = std::env::temp_dir().join(format!("worker-health-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(WORKER_READY_FILE);
        assert!(!heartbeat_is_fresh(&path, WORKER_READY_MAX_AGE));
        touch_heartbeat(Some(&directory), WORKER_READY_FILE);
        assert!(heartbeat_is_fresh(&path, WORKER_READY_MAX_AGE));
        std::thread::sleep(Duration::from_millis(20));
        assert!(!heartbeat_is_fresh(&path, Duration::from_millis(1)));
        std::fs::remove_dir_all(&directory).unwrap();
    }
    #[test]
    /// A database outage must not restart the worker: liveness outlasts the retry ceiling.
    fn liveness_tolerates_the_longest_retry() {
        assert!(WORKER_LIVE_MAX_AGE > Duration::from_secs(60));
        assert!(WORKER_READY_MAX_AGE < WORKER_LIVE_MAX_AGE);
    }
    #[test]
    /// Extreme configuration never produces a tight retry loop or unbounded waiting.
    fn processor_retry_clamps_initial_delay() {
        assert_eq!(
            RetryBackoff::new(Duration::ZERO).failure(),
            Duration::from_millis(100)
        );
        assert_eq!(
            RetryBackoff::new(Duration::MAX).failure(),
            Duration::from_secs(60)
        );
    }
}

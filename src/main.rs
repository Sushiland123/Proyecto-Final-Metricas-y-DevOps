//! Composition root and operational CLI. Migrations are explicit, not run on startup.
use event_service::{
    MIGRATOR,
    api::{self, AppState},
    config::Settings,
    postgres::PostgresRepository,
    service::EventService,
};
use std::time::Duration;
use std::{process::ExitCode, sync::Arc};
use utoipa::OpenApi;

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
    if !matches!(command.as_str(), "serve" | "migrate" | "process" | "drain") {
        return Err("Usage: event-service [serve|migrate|openapi|process|drain]");
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
        let worker = async {
            let mut backoff =
                RetryBackoff::new(Duration::from_millis(delay).max(settings.retry_backoff));
            loop {
                let mut wait = Duration::from_millis(delay);
                match store.process(batch_size).await {
                    Ok(0) if command == "drain" => return Ok(()),
                    Ok(count) => {
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

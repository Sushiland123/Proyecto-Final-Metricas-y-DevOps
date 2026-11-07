//! Composition root and operational CLI. Migrations are explicit, not run on startup.
use event_service::{
    MIGRATOR,
    api::{self, AppState},
    config::Settings,
    postgres::PostgresRepository,
    service::EventService,
};
use std::{process::ExitCode, sync::Arc};
use utoipa::OpenApi;

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
    if !matches!(command.as_str(), "serve" | "migrate") {
        return Err("Usage: event-service [serve|migrate|openapi]");
    }
    let settings = Settings::from_env();
    let repository = PostgresRepository::new(&settings);
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
    let state = AppState {
        settings: Arc::new(settings),
        service: EventService::new(Arc::new(repository)),
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

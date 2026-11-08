//! Destructive only to the dedicated test container this program creates.
//! Verifies same-process recovery, then full API + DB restart with retained volume.
use reqwest::{Client, StatusCode, Url};
use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    fs::File,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct ApiProcess(Child);
impl Drop for ApiProcess {
    /// Reap only the API child owned by this experiment, including error paths.
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct TestContainer {
    engine: String,
    name: String,
}
impl TestContainer {
    /// Execute a checked engine operation against the uniquely named test container.
    fn command(&self, args: &[&str]) -> Result<()> {
        if !Command::new(&self.engine).args(args).status()?.success() {
            return Err("container operation failed".into());
        }
        Ok(())
    }
}
impl Drop for TestContainer {
    /// Stop the experiment-owned container while retaining its volume for inspection.
    fn drop(&mut self) {
        let _ = Command::new(&self.engine)
            .args(["stop", &self.name])
            .output();
    }
}

/// Require configuration without copying its secret value into error messages.
fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{name} is required").into())
}

/// Spawn the real binary on an isolated listener and capture both log streams as local evidence.
fn start_api(binary: &PathBuf, address: &str) -> Result<ApiProcess> {
    std::fs::create_dir_all("artifacts")?;
    let log = File::options()
        .create(true)
        .append(true)
        .open("artifacts/system-api.log")?;
    Ok(ApiProcess(
        Command::new(binary)
            .arg("serve")
            .env("BIND_ADDR", address)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    ))
}

/// Bound readiness waits and fail promptly if the owned API process terminates.
async fn wait_ready(client: &Client, base: &str, process: &mut ApiProcess) -> Result<()> {
    for _ in 0..90 {
        if process.0.try_wait()?.is_some() {
            return Err("API exited; inspect artifacts/system-api.log".into());
        }
        if client
            .get(format!("{base}/health/ready"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("readiness timeout".into())
}

/// Wait for the dedicated test DB and apply embedded migrations without disclosing its DSN.
async fn migrate(repository: &event_service::postgres::PostgresRepository) -> Result<()> {
    for _ in 0..60 {
        if event_service::MIGRATOR
            .run(
                repository
                    .pool()
                    .map_err(|_| "invalid database configuration")?,
            )
            .await
            .is_ok()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("migration timeout".into())
}

#[tokio::main]
/// Verify same-process recovery, full restart durability and redaction using dedicated real processes.
/// Refuse nonlocal/non-test DB configuration and never delete existing application storage.
async fn main() -> Result<()> {
    let url = Url::parse(&required("DATABASE_URL")?).map_err(|_| "invalid DATABASE_URL")?;
    let db = required("POSTGRES_DB")?;
    let port = required("POSTGRES_PORT")?.parse::<u16>()?;
    let user = required("POSTGRES_USER")?;
    let _password = required("POSTGRES_PASSWORD")?;
    let producer = required("EVENT_PRODUCER_KEY")?;
    let analyst = required("EVENT_ANALYST_KEY")?;
    if !matches!(url.scheme(), "postgres" | "postgresql")
        || url.host_str() != Some("127.0.0.1")
        || url.port() != Some(port)
        || url.path() != format!("/{db}")
        || url.username() != user
        || !(db.starts_with("test_") || db.ends_with("_test"))
    {
        return Err(
            "DATABASE_URL must match the dedicated local Compose-style *_test configuration".into(),
        );
    }
    let name = format!("pfmdo-system-{}", Uuid::new_v4().simple());
    let container = TestContainer {
        engine: env::var("CONTAINER_ENGINE").unwrap_or_else(|_| "docker".into()),
        name,
    };
    let volume = format!("{}-data", container.name);
    let mapping = format!("127.0.0.1:{port}:5432");
    let mount = format!("{volume}:/var/lib/postgresql/data");
    container.command(&[
        "run",
        "-d",
        "--name",
        &container.name,
        "-e",
        "POSTGRES_USER",
        "-e",
        "POSTGRES_PASSWORD",
        "-e",
        "POSTGRES_DB",
        "-p",
        &mapping,
        "-v",
        &mount,
        "docker.io/library/postgres:16-alpine",
    ])?;
    let repository = event_service::postgres::PostgresRepository::new(
        &event_service::config::Settings::from_env(),
    );
    migrate(&repository).await?;
    let binary = env::var_os("EVENT_SERVICE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = env::current_exe().unwrap_or_default();
            path.pop();
            path.pop();
            path.push("event-service");
            path
        });
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let base = format!("http://{address}");
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let mut process = start_api(&binary, &address)?;
    wait_ready(&client, &base, &mut process).await?;
    let source = format!("system-{}", Uuid::new_v4().simple());
    let input = json!({"source": source, "external_id": "durable", "type": "temperature", "occurred_at": "2026-10-21T12:00:00Z", "payload": {"value":23.5}});
    let response = client
        .post(format!("{base}/v1/events"))
        .header("X-API-Key", &producer)
        .json(&input)
        .send()
        .await?;
    if response.status() != StatusCode::CREATED {
        return Err("event not created".into());
    }
    let original: Value = response.json().await?;
    // ESC3 durability includes device configuration, pending inbox, completed work,
    // rejection receipts and business alerts, not just the legacy events table.
    let device = format!("esc3-{}", Uuid::new_v4().simple());
    client.post(format!("{base}/v1/devices")).header("X-API-Key", &producer)
        .json(&json!({"id":device,"facility_id":"restart-test","limits":{"temperature_min":0,"temperature_max":30,"humidity_min":0,"humidity_max":100}}))
        .send().await?.error_for_status()?;
    let mut esc3_receipts = Vec::new();
    for n in 0..2 {
        let receipt: Value = client.post(format!("{base}/v1/readings")).header("X-API-Key", &producer)
            .json(&json!({"device_id":device,"event_id":format!("e-{n}"),"occurred_at":"2040-01-01T00:00:00Z","temperature":40,"humidity":50,"state":"running"}))
            .send().await?.error_for_status()?.json().await?;
        esc3_receipts.push(receipt["id"].as_str().ok_or("missing receipt")?.to_owned());
    }
    let store = event_service::scenario::store::Store {
        pool: repository.pool().map_err(|_| "missing pool")?.clone(),
    };
    if store
        .process(1)
        .await
        .map_err(|_| "ESC3 processing failed")?
        != 1
    {
        return Err("expected one processed reading".into());
    }
    let rejected = client
        .post(format!("{base}/v1/readings"))
        .header("X-API-Key", &producer)
        .json(&json!({"device_id":device}))
        .send()
        .await?;
    if rejected.status() != StatusCode::UNPROCESSABLE_ENTITY {
        return Err("expected durable rejection".into());
    }
    let rejected: Value = rejected.json().await?;
    esc3_receipts.push(
        rejected["id"]
            .as_str()
            .ok_or("missing rejection receipt")?
            .to_owned(),
    );
    let period = "from=2040-01-01T00:00:00Z&to=2040-01-02T00:00:00Z";
    let mut esc3_paths = vec![
        format!("/v1/devices/{device}"),
        format!("/v1/devices/{device}/readings?{period}"),
        format!("/v1/devices/{device}/statistics?{period}"),
        format!("/v1/devices/{device}/alerts?{period}"),
        format!("/v1/reception/counts?device_id={device}"),
        format!("/v1/rejections?device_id={device}"),
    ];
    for id in esc3_receipts {
        esc3_paths.push(format!("/v1/receipts/{id}"));
    }
    let mut esc3_before = Vec::new();
    for path in &esc3_paths {
        esc3_before.push(
            client
                .get(format!("{base}{path}"))
                .header("X-API-Key", &analyst)
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?,
        );
    }
    let marker = format!("sensitive-test-{}", Uuid::new_v4().simple());
    let mut duplicate = input.clone();
    duplicate["payload"] = json!({"secret": marker});
    let conflict = client
        .post(format!("{base}/v1/events"))
        .header("X-API-Key", &producer)
        .header("X-Request-ID", "system-conflict-1")
        .json(&duplicate)
        .send()
        .await?;
    if conflict.status() != StatusCode::CONFLICT
        || conflict
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            != Some("system-conflict-1")
        || conflict.text().await?.contains(&marker)
    {
        return Err("unsafe conflict response".into());
    }
    let invalid = client
        .get(format!("{base}/v1/events/{marker}"))
        .header("X-API-Key", &analyst)
        .send()
        .await?;
    if invalid.status() != StatusCode::UNPROCESSABLE_ENTITY
        || invalid.text().await?.contains(&marker)
    {
        return Err("unsafe path validation response".into());
    }
    let query = format!(
        "{base}/v1/analysis/summary?from=2026-10-21T00:00:00Z&to=2026-10-22T00:00:00Z&source={source}"
    );
    let pid = process.0.id();
    container.command(&["stop", &container.name])?;
    let down = client
        .get(&query)
        .header("X-API-Key", &analyst)
        .send()
        .await?;
    if down.status() != StatusCode::SERVICE_UNAVAILABLE {
        return Err("expected 503 while database is down".into());
    }
    if client
        .get(format!("{base}/health/live"))
        .send()
        .await?
        .status()
        != StatusCode::OK
    {
        return Err("liveness failed during outage".into());
    }
    container.command(&["start", &container.name])?;
    wait_ready(&client, &base, &mut process).await?;
    if process.0.id() != pid
        || client
            .get(&query)
            .header("X-API-Key", &analyst)
            .send()
            .await?
            .status()
            != StatusCode::OK
    {
        return Err("same-process recovery failed".into());
    }
    println!("PASS: 200 -> 503 -> 200 without restarting API");
    drop(process);
    container.command(&["stop", &container.name])?;
    container.command(&["start", &container.name])?;
    migrate(&repository).await?;
    migrate(&repository).await?;
    let mut process = start_api(&binary, &address)?;
    wait_ready(&client, &base, &mut process).await?;
    let id = original["id"].as_str().ok_or("missing event UUID")?;
    let restored: Value = client
        .get(format!("{base}/v1/events/{id}"))
        .header("X-API-Key", &analyst)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if original != restored {
        return Err("event changed after restart".into());
    }
    let summary: Value = client
        .get(&query)
        .header("X-API-Key", &analyst)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if summary["total_events"] != 1 {
        return Err("summary does not reflect durable event".into());
    }
    println!("PASS: event {id} persisted after API + PostgreSQL restart");
    for (path, before) in esc3_paths.iter().zip(&esc3_before) {
        let after: Value = client
            .get(format!("{base}{path}"))
            .header("X-API-Key", &analyst)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if &after != before {
            return Err("ESC3 durable state changed after API + PostgreSQL restart".into());
        }
    }
    let recovered = store
        .counters(Some(&device))
        .await
        .map_err(|_| "ESC3 counters failed")?;
    if recovered.processed != 1 || recovered.pending != 1 || recovered.rejected != 1 {
        return Err("ESC3 restart counters mismatch".into());
    }
    store.process(100).await.map_err(|_| "ESC3 drain failed")?;
    if store
        .counters(Some(&device))
        .await
        .map_err(|_| "ESC3 counters failed")?
        .pending
        != 0
    {
        return Err("ESC3 backlog did not drain".into());
    }
    println!(
        "PASS: ESC3 devices, limits, readings, rejection receipts, alerts and pending work survived API + PostgreSQL restart; backlog drained to zero"
    );
    let logs = std::fs::read_to_string("artifacts/system-api.log")?;
    if [
        producer.as_str(),
        analyst.as_str(),
        marker.as_str(),
        source.as_str(),
        url.as_str(),
    ]
    .iter()
    .any(|secret| logs.contains(*secret))
    {
        return Err("sensitive data appeared in API logs".into());
    }
    println!(
        "PASS: failure responses and API logs do not disclose submitted secrets or URL values"
    );
    println!(
        "Retained stopped test container {} and volume {volume}; remove explicitly when no longer needed.",
        container.name
    );
    Ok(())
}

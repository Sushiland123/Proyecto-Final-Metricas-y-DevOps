//! Reproducible ESC3 load generator. No monitoring stack or deployment changes.
//! cargo run --locked --example scenario_load -- burst|extra|sustained|custom [--name value ...]
use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};

struct Attempt {
    status: u16,
    body: Value,
    latency_ms: f64,
    lag_ms: f64,
}
/// Submit one scheduled attempt and preserve uncertain outcomes without logging credentials.
async fn send(
    client: Client,
    base: String,
    key: String,
    body: Value,
    due: tokio::time::Instant,
) -> Attempt {
    let lag_ms = tokio::time::Instant::now()
        .saturating_duration_since(due)
        .as_secs_f64()
        * 1000.0;
    let start = Instant::now();
    match client
        .post(format!("{base}/v1/readings"))
        .header("X-API-Key", key)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => Attempt {
            status: response.status().as_u16(),
            body: response.json().await.unwrap_or(Value::Null),
            latency_ms: start.elapsed().as_secs_f64() * 1000.0,
            lag_ms,
        },
        Err(_) => Attempt {
            status: 0,
            body: Value::Null,
            latency_ms: start.elapsed().as_secs_f64() * 1000.0,
            lag_ms,
        },
    }
}
/// Generate deterministic fictitious measurements and UTC timestamps for a numbered event.
fn payload(device: &str, n: usize, seed: u64) -> Value {
    // Fixed integer mixing, independent of platform RNG; same seed => same measurements.
    let value = (n as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(seed);
    json!({"device_id":device,"event_id":format!("e-{n}"),
        "occurred_at":(Utc.with_ymd_and_hms(2040,1,1,0,0,0).single().unwrap_or_default()+chrono::Duration::seconds(n as i64)).to_rfc3339(),
        "temperature":10.0+(value%400) as f64/10.0,"humidity":30.0+(value%600) as f64/10.0,"state":"running"})
}
/// Compute an observed order-statistic percentile, returning zero for an empty population.
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get(((values.len().saturating_sub(1)) as f64 * p).ceil() as usize)
        .copied()
        .unwrap_or(0.0)
}
#[tokio::main]
/// Run a bounded reproducible profile, verify accepted UUIDs and print a safe JSON report.
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let profile = args.first().map(String::as_str).unwrap_or("burst");
    if !matches!(profile, "burst" | "extra" | "sustained" | "custom")
        || (args.len().saturating_sub(1)) % 2 != 0
    {
        return Err("Usage: scenario_load burst|extra|sustained|custom [--seed N] [--run IDENTIFIER] [--count N] [--rate N] [--invalid N] [--duplicates N] [--late N]".into());
    }
    let mut options = BTreeMap::new();
    for pair in args.get(1..).unwrap_or_default().chunks_exact(2) {
        if !matches!(
            pair[0].as_str(),
            "--seed" | "--run" | "--count" | "--rate" | "--invalid" | "--duplicates" | "--late"
        ) || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err("Unknown or repeated option".into());
        }
    }
    let number = |name: &str, default: usize| -> Result<usize, Box<dyn std::error::Error>> {
        Ok(options
            .get(name)
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(default))
    };
    let seed = number("--seed", 3)? as u64;
    let count = number("--count", if profile == "sustained" { 2400 } else { 1000 })?;
    let rate = number("--rate", if profile == "sustained" { 20 } else { 0 })?;
    let invalid = number("--invalid", if profile == "sustained" { 0 } else { 20 })?;
    let duplicates = number("--duplicates", if profile == "extra" { 50 } else { 0 })?;
    let late = number("--late", if profile == "extra" { 50 } else { 0 })?;
    if count == 0
        || count > 100_000
        || rate > 10_000
        || invalid > 100_000
        || duplicates > 100_000
        || late > count
    {
        return Err("Invalid load bounds".into());
    }
    if profile == "sustained"
        && (count != 2400 || rate != 20 || invalid != 0 || duplicates != 0 || late != 0)
    {
        return Err(
            "Use custom to change the compulsory sustained profile (2400, 20/s, 120s)".into(),
        );
    }
    let run = options
        .get("--run")
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
    let device = format!("load-{run}");
    if !event_service::domain::identifier(&device) {
        return Err("Invalid --run identifier (max 59 ASCII bytes)".into());
    }
    let base = std::env::var("ESC3_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8000".into())
        .trim_end_matches('/')
        .to_owned();
    let producer = std::env::var("EVENT_PRODUCER_KEY")?;
    let analyst = std::env::var("EVENT_ANALYST_KEY")?;
    // Never forward credentials across redirects, including to another host.
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let created=client.post(format!("{base}/v1/devices")).header("X-API-Key",&producer)
        .json(&json!({"id":device,"facility_id":"simulated-maintenance","limits":{"temperature_min":0,"temperature_max":35,"humidity_min":20,"humidity_max":80}})).send().await?;
    if created.status() != 201 {
        return Err("Device registration failed; use a new --run and a global producer key".into());
    }
    let started = tokio::time::Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    let mut attempts = Vec::new();
    // At most 64 in flight. If the target cannot keep up, schedule lag makes it visible.
    for n in 0..count {
        let due = started
            + Duration::from_secs_f64(if rate == 0 {
                0.0
            } else {
                n as f64 / rate as f64
            });
        tokio::time::sleep_until(due).await;
        if tasks.len() >= 64 {
            attempts.push(tasks.join_next().await.ok_or("missing task")??);
        }
        tasks.spawn(send(
            client.clone(),
            base.clone(),
            producer.clone(),
            payload(&device, n, seed),
            due,
        ));
    }
    if rate > 0 {
        tokio::time::sleep_until(started + Duration::from_secs_f64(count as f64 / rate as f64))
            .await;
    }
    let emission_seconds = started.elapsed().as_secs_f64();
    while let Some(result) = tasks.join_next().await {
        attempts.push(result?);
    }
    for n in 0..duplicates + late + invalid {
        let body = if n < duplicates {
            payload(&device, n % count, seed)
        } else if n < duplicates + late {
            let k = n - duplicates;
            let mut body = payload(&device, late - 1 - k, seed);
            body["event_id"] = json!(format!("late-{k}"));
            body
        } else {
            let k = n - duplicates - late;
            let mut body = payload(&device, count + k, seed);
            match k % 4 {
                0 => {
                    body.as_object_mut()
                        .ok_or("not an object")?
                        .remove("humidity");
                }
                1 => body["humidity"] = json!(101),
                2 => body["device_id"] = json!(format!("unknown-{run}")),
                _ => body["state"] = json!("invalid"),
            };
            body
        };
        attempts.push(
            send(
                client.clone(),
                base.clone(),
                producer.clone(),
                body,
                tokio::time::Instant::now(),
            )
            .await,
        );
    }
    let load_seconds = started.elapsed().as_secs_f64();
    let mut statuses = BTreeMap::<u16, usize>::new();
    let mut accepted_ids = HashSet::new();
    let mut receipt_ids = Vec::new();
    for attempt in &attempts {
        *statuses.entry(attempt.status).or_default() += 1;
        if attempt.status == 202 {
            accepted_ids.insert(
                attempt.body["reading_id"]
                    .as_str()
                    .ok_or("Missing reading id")?
                    .to_owned(),
            );
        }
        if let Some(id) = attempt.body["id"].as_str() {
            receipt_ids.push(id.to_owned());
        }
    }
    let mut durable = HashSet::new();
    for offset in (0..count + late).step_by(1000) {
        let response=client.get(format!("{base}/v1/devices/{device}/readings?from=2040-01-01T00:00:00Z&to=2041-01-01T00:00:00Z&limit=1000&offset={offset}"))
            .header("X-API-Key",&analyst).send().await?.error_for_status()?.json::<Vec<Value>>().await?;
        for row in response {
            durable.insert(
                row["id"]
                    .as_str()
                    .ok_or("Missing stored reading id")?
                    .to_owned(),
            );
        }
    }
    let lost = accepted_ids.difference(&durable).count();
    let counters = client
        .get(format!("{base}/v1/reception/counts?device_id={device}"))
        .header("X-API-Key", &analyst)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let mut latency: Vec<_> = attempts.iter().map(|a| a.latency_ms).collect();
    let mut lag: Vec<_> = attempts.iter().take(count).map(|a| a.lag_ms).collect();
    let passed = statuses.get(&202).copied().unwrap_or(0) == count + late
        && statuses.get(&200).copied().unwrap_or(0) == duplicates
        && statuses.get(&422).copied().unwrap_or(0) == invalid
        && lost == 0
        && (profile != "sustained"
            || emission_seconds <= 121.0 && percentile(&mut lag, 1.0) < 1000.0);
    let report = json!({"profile":profile,"seed":seed,"run":run,"device_id":device,
        "unique_requested":count+late,"duplicates_requested":duplicates,"late_requested":late,"invalid_requested":invalid,
        "emission_seconds":emission_seconds,"load_seconds":load_seconds,"target_rate":rate,"statuses":statuses,
        "latency_ms":{"p50":percentile(&mut latency,0.50),"p95":percentile(&mut latency,0.95),"max":percentile(&mut latency,1.0)},
        "schedule_lag_ms_max":percentile(&mut lag,1.0),"lost_accepted":lost,"uncertain_requests":statuses.get(&0).copied().unwrap_or(0),
        "counters":counters,"counter_scope":"device_id of this run; unknown-device rejections are only in global/unknown-device counts",
        "unknown_device_rejections_requested":(invalid+1)/4,"receipt_ids":receipt_ids,"passed":passed});
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !passed {
        return Err("Acceptance mismatch; inspect JSON report (network errors are uncertain, not automatically rejected)".into());
    }
    Ok(())
}

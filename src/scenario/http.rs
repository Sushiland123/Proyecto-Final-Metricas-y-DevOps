//! Specific ESC3 API. Legacy /v1/events remains available but is not ESC3 intake.
use super::{store::Store, *};
use crate::{
    auth::{self, Principal, Role},
    config::Settings,
    postgres::PostgresRepository,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use std::sync::Arc;
use utoipa::OpenApi;

#[derive(Clone)]
pub struct ScenarioState {
    settings: Arc<Settings>,
    store: Option<Store>,
}
impl ScenarioState {
    fn store(&self) -> Result<&Store, Error> {
        self.store.as_ref().ok_or(Error::Unavailable)
    }
}
struct Identity(Principal);
/// This bounded response is already safe and includes the durable rejection identifier.
#[derive(Clone)]
pub struct RecordedRejection;
impl FromRequestParts<ScenarioState> for Identity {
    type Rejection = Error;
    async fn from_request_parts(parts: &mut Parts, state: &ScenarioState) -> Result<Self, Error> {
        let mut keys = parts.headers.get_all("x-api-key").iter();
        let key = keys.next().and_then(|v| v.to_str().ok());
        if keys.next().is_some() {
            return Err(Error::Unauthenticated);
        }
        auth::authenticate(key, &state.settings).map(Self)
    }
}
pub fn router(settings: Arc<Settings>) -> Router {
    let store = PostgresRepository::new(&settings)
        .pool()
        .ok()
        .cloned()
        .map(|pool| Store { pool });
    Router::new()
        .route("/v1/devices", post(register).get(devices))
        .route("/v1/devices/{device}", get(device))
        .route("/v1/devices/{device}/limits", put(limits))
        .route("/v1/readings", post(receive))
        .route("/v1/receipts/{id}", get(receipt))
        .route("/v1/devices/{device}/readings", get(readings))
        .route("/v1/devices/{device}/statistics", get(aggregate))
        .route("/v1/devices/{device}/alerts", get(alerts))
        .route("/v1/reception/counts", get(counters))
        .route("/v1/rejections", get(rejections))
        .with_state(ScenarioState { settings, store })
}
fn valid_device(id: &str) -> Result<(), Error> {
    if identifier(id) {
        Ok(())
    } else {
        Err(Error::InvalidInput)
    }
}

#[utoipa::path(post, path="/v1/devices", request_body=DeviceCreate, security(("api_key"=[])), responses((status=201, body=Device)))]
async fn register(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    body: Result<Json<DeviceCreate>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<Device>), Error> {
    who.require_device_admin()?;
    let Json(input) = body.map_err(|_| Error::InvalidInput)?;
    Ok((
        StatusCode::CREATED,
        Json(state.store()?.register(&input).await?),
    ))
}
#[utoipa::path(get, path="/v1/devices", params(Page), security(("api_key"=[])), responses((status=200, body=Vec<Device>)))]
async fn devices(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<Device>>, Error> {
    who.require(Role::Analyst)?;
    let Query(page) = query.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.devices(&page).await?))
}
#[utoipa::path(get, path="/v1/devices/{device}", params(("device"=String,Path)), security(("api_key"=[])), responses((status=200,body=Device)))]
async fn device(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    Path(id): Path<String>,
) -> Result<Json<Device>, Error> {
    who.require(Role::Analyst)?;
    valid_device(&id)?;
    Ok(Json(state.store()?.device(&id).await?))
}
#[utoipa::path(put, path="/v1/devices/{device}/limits", params(("device"=String,Path)), request_body=Limits, security(("api_key"=[])), responses((status=200,body=Device)))]
async fn limits(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    Path(id): Path<String>,
    body: Result<Json<Limits>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Device>, Error> {
    who.require_device_admin()?;
    valid_device(&id)?;
    let Json(input) = body.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.limits(&id, &input).await?))
}
#[utoipa::path(post, path="/v1/readings", request_body=ReadingCreate, security(("api_key"=[])), responses((status=202,body=Receipt),(status=200,description="Duplicate; original reading_id, no new work",body=Receipt),(status=422,description="Durably recorded rejection",body=Receipt)))]
async fn receive(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Error> {
    who.require(Role::Producer)?;
    // Parse ourselves to record malformed JSON and missing/invalid typed fields.
    let value = serde_json::from_slice::<serde_json::Value>(&body).ok();
    let device = value
        .as_ref()
        .and_then(|v| v.get("device_id"))
        .and_then(|v| v.as_str())
        .filter(|id| identifier(id));
    if let Some(id) = device {
        who.require_source(id)?;
    }
    let is_json = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|m| m.trim() == "application/json")
        });
    let input = if is_json {
        serde_json::from_slice::<ReadingCreate>(&body).ok()
    } else {
        None
    };
    let store = state.store()?;
    let receipt = match input {
        Some(input) => store.receive(&input).await?,
        None => store.reject(device, "invalid_format").await?,
    };
    let status = match receipt.outcome.as_str() {
        "accepted" => StatusCode::ACCEPTED,
        "duplicate" => StatusCode::OK,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    let mut response = (status, Json(receipt)).into_response();
    if status == StatusCode::UNPROCESSABLE_ENTITY {
        response.extensions_mut().insert(RecordedRejection);
    }
    Ok(response)
}
#[utoipa::path(get, path="/v1/receipts/{id}", params(("id"=Uuid,Path)), security(("api_key"=[])), responses((status=200,description="Durable reception receipt")))]
async fn receipt(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    id: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
) -> Result<Json<serde_json::Value>, Error> {
    who.require(Role::Analyst)?;
    let Path(id) = id.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.receipt(id).await?))
}
#[utoipa::path(get, path="/v1/devices/{device}/readings", params(("device"=String,Path),Period), security(("api_key"=[])), responses((status=200,body=Vec<Reading>)))]
async fn readings(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    Path(id): Path<String>,
    query: Result<Query<Period>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<Reading>>, Error> {
    who.require(Role::Analyst)?;
    valid_device(&id)?;
    let Query(period) = query.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.readings(&id, &period).await?))
}
#[utoipa::path(get, path="/v1/devices/{device}/statistics", params(("device"=String,Path),Period), security(("api_key"=[])), responses((status=200,body=Aggregate)))]
async fn aggregate(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    Path(id): Path<String>,
    query: Result<Query<Period>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Aggregate>, Error> {
    who.require(Role::Analyst)?;
    valid_device(&id)?;
    let Query(period) = query.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.aggregate(&id, &period).await?))
}
#[utoipa::path(get, path="/v1/devices/{device}/alerts", params(("device"=String,Path),Period), security(("api_key"=[])), responses((status=200,body=Vec<Alert>)))]
async fn alerts(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    Path(id): Path<String>,
    query: Result<Query<Period>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<Alert>>, Error> {
    who.require(Role::Analyst)?;
    valid_device(&id)?;
    let Query(period) = query.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.alerts(&id, &period).await?))
}
#[utoipa::path(get, path="/v1/reception/counts", params(Page), security(("api_key"=[])), responses((status=200,body=Counters)))]
async fn counters(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Counters>, Error> {
    who.require(Role::Analyst)?;
    let Query(page) = query.map_err(|_| Error::InvalidInput)?;
    page.validate()?;
    Ok(Json(
        state.store()?.counters(page.device_id.as_deref()).await?,
    ))
}
#[utoipa::path(get, path="/v1/rejections", params(Page), security(("api_key"=[])), responses((status=200,description="Safe rejection ledger, paginated")))]
async fn rejections(
    State(state): State<ScenarioState>,
    Identity(who): Identity,
    query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<serde_json::Value>>, Error> {
    who.require(Role::Analyst)?;
    let Query(page) = query.map_err(|_| Error::InvalidInput)?;
    Ok(Json(state.store()?.rejections(&page).await?))
}
#[derive(OpenApi)]
#[openapi(paths(
    register, devices, device, limits, receive, receipt, readings, aggregate, alerts, counters,
    rejections
))]
pub struct ScenarioDoc;

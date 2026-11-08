#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use support::*;
use tower::ServiceExt;

#[tokio::test]
/// Verify public probes, RBAC and sanitized failures, including NUL rejection without a database.
async fn health_and_uniform_safe_errors() {
    let app = app(settings());
    assert_eq!(
        request(&app, "GET", "/health/live", None, None).await.1,
        json!({"status": "alive"})
    );
    assert_error(
        &request(&app, "GET", "/health/ready", None, None).await,
        503,
        "DEPENDENCY_UNAVAILABLE",
    );
    assert_error(
        &request(&app, "GET", "/secret-url", None, None).await,
        404,
        "NOT_FOUND",
    );
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events",
            None,
            Some(event("sensor-a", "id")),
        )
        .await,
        401,
        "UNAUTHENTICATED",
    );
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events",
            Some("test-analyst"),
            Some(event("sensor-a", "id")),
        )
        .await,
        403,
        "FORBIDDEN",
    );
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events",
            Some("scoped-a"),
            Some(event("sensor-b", "id")),
        )
        .await,
        403,
        "FORBIDDEN",
    );
    let mut invalid = event("sensor-a", "id");
    invalid["payload"] = json!(["secret-input"]);
    let response = request(
        &app,
        "POST",
        "/v1/events",
        Some("test-producer"),
        Some(invalid),
    )
    .await;
    assert_error(&response, 422, "INVALID_INPUT");
    assert!(!response.1.to_string().contains("secret-input"));
    for payload in [
        json!({"value":"\u{0000}"}),
        json!({"nested":[{"\u{0000}":1}]}),
    ] {
        let mut invalid = event("sensor-a", "id");
        invalid["payload"] = payload;
        assert_error(
            &request(
                &app,
                "POST",
                "/v1/events",
                Some("test-producer"),
                Some(invalid),
            )
            .await,
            422,
            "INVALID_INPUT",
        );
    }
    assert_error(
        &request(
            &app,
            "GET",
            "/v1/events/not-a-uuid",
            Some("test-analyst"),
            None,
        )
        .await,
        422,
        "INVALID_INPUT",
    );
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events",
            Some("test-producer"),
            Some(event("sensor-a", "id")),
        )
        .await,
        503,
        "DEPENDENCY_UNAVAILABLE",
    );
}

#[tokio::test]
/// Confirm intra-batch duplicate detection wins before attempting unavailable persistence.
async fn duplicate_batch_is_rejected_before_storage() {
    let app = app(settings());
    let event = event("sensor-a", "id");
    assert_error(
        &request(
            &app,
            "POST",
            "/v1/events/batch",
            Some("test-producer"),
            Some(json!({"events": [event, event]})),
        )
        .await,
        409,
        "DUPLICATE",
    );
}

#[tokio::test]
/// Normalize extractor failures and retain HTTP method semantics including the Allow header.
async fn rejects_malformed_json_unknown_query_and_wrong_methods() {
    let app = app(settings());
    for path in [
        "/v1/events?from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z&extra=secret",
        "/v1/analysis/counts?from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z&group_by=payload",
        "/v1/analysis/timeseries?from=2026-01-01T00:00:00Z&to=2026-02-01T00:00:00Z&bucket=minute",
    ] {
        assert_error(
            &request(&app, "GET", path, Some("test-analyst"), None).await,
            422,
            "INVALID_INPUT",
        );
    }
    let request = Request::builder()
        .method("POST")
        .uri("/v1/events")
        .header("x-api-key", "test-producer")
        .header("content-type", "application/json")
        .body(Body::from("{\"secret-input\":"))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), 422);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains("secret-input"));
    assert_error(
        &support::request(&app, "DELETE", "/v1/events", None, None).await,
        405,
        "INVALID_INPUT",
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let allow = response.headers()["allow"].to_str().unwrap();
    assert!(allow.contains("GET") && allow.contains("POST"));
}

#[tokio::test]
/// Validate single correlation headers and prevent ambiguous multiple API-key authentication.
async fn request_id_is_preserved_or_replaced_and_duplicate_keys_rejected() {
    let app = app(settings());
    for ids in [
        vec!["safe-123"],
        vec!["contains spaces?"],
        vec!["first", "second"],
        vec!["a"; 0],
    ] {
        let mut request = Request::builder().uri("/missing");
        for id in &ids {
            request = request.header("x-request-id", *id);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        if ids == ["safe-123"] {
            assert_eq!(id, "safe-123");
        } else {
            uuid::Uuid::parse_str(&id).unwrap();
        }
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["error"]["request_id"], id);
    }
    let request = Request::builder()
        .uri("/v1/events/00000000-0000-0000-0000-000000000000")
        .header("x-api-key", "test-analyst")
        .header("x-api-key", "test-producer")
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.oneshot(request).await.unwrap().status(), 401);
}

#[tokio::test]
/// Ensure every protected operation exports security metadata and shared error responses.
async fn generated_openapi_documents_all_routes_and_security() {
    let app = app(settings());
    let response = request(&app, "GET", "/openapi.json", None, None).await;
    assert_eq!(response.0, 200);
    assert_eq!(response.1["paths"].as_object().unwrap().len(), 18);
    assert_eq!(
        response.1["components"]["securitySchemes"]["api_key"]["name"],
        "X-API-Key"
    );
    for (path, methods) in response.1["paths"].as_object().unwrap() {
        if path.starts_with("/v1/") {
            for operation in methods.as_object().unwrap().values() {
                assert!(operation["security"].is_array());
                assert!(operation["responses"]["503"].is_object());
            }
        }
    }
}

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_xnor::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

/// Mock dependency answering `POST /` with a fixed status + body.
async fn spawn_mock(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(xor_url: &str, not_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            xor_url: xor_url.to_string(),
            not_url: not_url.to_string(),
        },
    )
}

async fn eval(xor_url: &str, not_url: &str, a: Value, b: Value) -> (StatusCode, Value) {
    let res = app(xor_url, not_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "a": a, "b": b }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

const DEAD_URL: &str = "http://127.0.0.1:1";

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn index_ok() {
    assert_eq!(status_of("/").await, StatusCode::OK);
}

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

/// Drive one truth-table row. `xor_result` is what the mock `srvcs-xor` returns
/// for `a XOR b`; `not_result` is what mock `srvcs-not` returns for `NOT(xor)`,
/// which is the expected final `result`.
async fn case(a: bool, b: bool, xor_result: bool, not_result: bool) {
    let xor = spawn_mock(StatusCode::OK, json!({ "result": xor_result })).await;
    let not = spawn_mock(StatusCode::OK, json!({ "result": not_result })).await;
    let (status, body) = eval(&xor, &not, json!(a), json!(b)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["a"], json!(a));
    assert_eq!(body["b"], json!(b));
    assert_eq!(
        body["result"],
        json!(not_result),
        "xnor({a}, {b}) should be {not_result}"
    );
}

#[tokio::test]
async fn truth_table_false_false_is_true() {
    // a XOR b = false, NOT = true
    case(false, false, false, true).await;
}

#[tokio::test]
async fn truth_table_false_true_is_false() {
    // a XOR b = true, NOT = false
    case(false, true, true, false).await;
}

#[tokio::test]
async fn truth_table_true_false_is_false() {
    // a XOR b = true, NOT = false
    case(true, false, true, false).await;
}

#[tokio::test]
async fn truth_table_true_true_is_true() {
    // a XOR b = false, NOT = true
    case(true, true, false, true).await;
}

#[tokio::test]
async fn forwards_invalid_input_from_xor() {
    let xor = spawn_mock(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "operand is not a boolean" }),
    )
    .await;
    let not = spawn_mock(StatusCode::OK, json!({ "result": true })).await;
    let (status, _) = eval(&xor, &not, json!("nope"), json!(true)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn degrades_when_xor_is_unreachable() {
    let not = spawn_mock(StatusCode::OK, json!({ "result": true })).await;
    let (status, body) = eval(DEAD_URL, &not, json!(true), json!(false)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-xor");
}

#[tokio::test]
async fn degrades_when_not_is_unreachable() {
    let xor = spawn_mock(StatusCode::OK, json!({ "result": true })).await;
    let (status, body) = eval(&xor, DEAD_URL, json!(true), json!(false)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-not");
}

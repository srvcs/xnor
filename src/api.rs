use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-xnor";
pub const CONCERN: &str = "logic: NOT XOR (equivalence)";
pub const DEPENDS_ON: &[&str] = &["srvcs-xor", "srvcs-not"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub xor_url: String,
    pub not_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    #[schema(value_type = Object)]
    pub a: Value,
    #[schema(value_type = Object)]
    pub b: Value,
}

#[derive(Serialize, ToSchema)]
pub struct EvalResponse {
    #[schema(value_type = Object)]
    pub a: Value,
    #[schema(value_type = Object)]
    pub b: Value,
    pub result: bool,
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for invalid
/// input from a leaf dependency).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// Ask one boolean dependency for its verdict, mapping its failures to the
/// response this service should return.
async fn ask(url: &str, payload: &Value, dependency: &str) -> Result<bool, Response> {
    match client::call(url, payload).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body.get("result").and_then(Value::as_bool).unwrap_or(false)),
        // Invalid input — forward the leaf's rejection unchanged.
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — exclusive NOR (logical equivalence) of two boolean operands.
///
/// This service does no logic of its own. It composes `srvcs-xor` and
/// `srvcs-not`: `xnor = NOT(a XOR b)`, which is true exactly when `a == b`.
/// Input validation (operands must be booleans) propagates as a `422` from the
/// leaf dependencies.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = EvalResponse),
        (status = 422, description = "an operand is not a valid boolean (forwarded from a dependency)"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    // x = a XOR b
    let x = match ask(
        &deps.xor_url,
        &json!({ "a": req.a, "b": req.b }),
        "srvcs-xor",
    )
    .await
    {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    // result = NOT x
    let result = match ask(&deps.not_url, &json!({ "value": x }), "srvcs-not").await {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    (
        StatusCode::OK,
        Json(json!({ "a": req.a, "b": req.b, "result": result })),
    )
        .into_response()
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, EvalResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-xnor");
        assert_eq!(info.depends_on, vec!["srvcs-xor", "srvcs-not"]);
    }
}

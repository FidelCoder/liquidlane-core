use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    domain::{CreateDepositRequest, CreateLiquidityRequest},
    store::AppStore,
};

#[derive(Clone)]
pub struct AppState {
    pub environment: String,
    pub store: Arc<AppStore>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/vault", get(vault_summary))
        .route("/deposits", get(list_deposits).post(create_deposit))
        .route("/liquidity/quote", post(create_quote))
        .route(
            "/liquidity/requests",
            get(list_liquidity_requests).post(create_liquidity_request),
        )
        .route("/liquidity/requests/{id}/deploy", post(deploy_liquidity))
        .route("/activity", get(activity))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    environment: String,
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "liquidlane-core",
        environment: state.environment,
    })
}

#[derive(Deserialize)]
struct VaultQuery {
    asset: Option<String>,
}

async fn vault_summary(
    State(state): State<AppState>,
    Query(query): Query<VaultQuery>,
) -> impl IntoResponse {
    Json(state.store.vault_summary(query.asset).await)
}

async fn list_deposits(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.store.deposits().await)
}

async fn create_deposit(
    State(state): State<AppState>,
    Json(request): Json<CreateDepositRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_deposit(request).await?),
    ))
}

async fn create_quote(
    State(state): State<AppState>,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.quote(&request).await?))
}

async fn list_liquidity_requests(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.store.liquidity_requests().await)
}

async fn create_liquidity_request(
    State(state): State<AppState>,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_liquidity_request(request).await?),
    ))
}

async fn deploy_liquidity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.deploy_liquidity(id).await?))
}

async fn activity(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.store.activity().await)
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    fn test_app() -> Router {
        router(AppState {
            environment: "test".to_string(),
            store: Arc::new(AppStore::new()),
        })
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();

        assert!(body.contains("\"status\":\"ok\""));
        assert!(body.contains("\"service\":\"liquidlane-core\""));
        assert!(body.contains("\"environment\":\"test\""));
    }

    #[tokio::test]
    async fn can_create_and_deploy_liquidity_request() {
        let app = test_app();
        let body = json!({
            "merchant_name": "Nova Wallet",
            "asset": "USDC",
            "amount": 5000,
            "duration_days": 30
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/liquidity/requests")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let response_body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = response_body["id"].as_str().unwrap();
        assert_eq!(response_body["status"], "requested");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/liquidity/requests/{id}/deploy"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let response_body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(response_body["status"], "deployed");
        assert!(
            response_body["channel_id"]
                .as_str()
                .unwrap()
                .starts_with("fiber-channel-")
        );
    }
}

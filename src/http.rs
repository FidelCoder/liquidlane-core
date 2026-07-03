use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use crate::{
    domain::{
        ChallengeRequest, ConnectWalletRequest, CreateDepositRequest, CreateLiquidityRequest, User,
        VerifyWalletRequest,
    },
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
        .route("/auth/challenge", post(create_challenge))
        .route("/auth/connect", post(connect_wallet))
        .route("/auth/verify", post(verify_wallet))
        .route("/me", get(me))
        .route("/dashboard", get(dashboard))
        .route("/deposits", post(create_deposit))
        .route("/liquidity/quote", post(create_quote))
        .route("/liquidity/requests", post(create_liquidity_request))
        .route("/liquidity/requests/{id}/deploy", post(deploy_liquidity))
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

async fn create_challenge(
    State(state): State<AppState>,
    Json(request): Json<ChallengeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_challenge(request).await?),
    ))
}

async fn connect_wallet(
    State(state): State<AppState>,
    Json(request): Json<ConnectWalletRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.connect_wallet(request).await?),
    ))
}

async fn verify_wallet(
    State(state): State<AppState>,
    Json(request): Json<VerifyWalletRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.verify_wallet(request).await?),
    ))
}

async fn me(AuthedUser(user): AuthedUser) -> impl IntoResponse {
    Json(crate::domain::UserProfile::from(&user))
}

#[derive(Deserialize)]
struct DashboardQuery {
    asset: Option<String>,
}

async fn dashboard(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Query(query): Query<DashboardQuery>,
) -> impl IntoResponse {
    Json(state.store.dashboard(&user, query.asset).await)
}

async fn create_deposit(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateDepositRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_deposit(&user, request).await?),
    ))
}

async fn create_quote(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.quote(&user, &request).await?))
}

async fn create_liquidity_request(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Json(request): Json<CreateLiquidityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.create_liquidity_request(&user, request).await?),
    ))
}

async fn deploy_liquidity(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.deploy_liquidity(&user, id).await?))
}

struct AuthedUser(User);

impl FromRequestParts<AppState> for AuthedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("missing authorization token"))?;
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::unauthorized("authorization token must use Bearer scheme"))?;

        let user = state
            .store
            .user_by_token(token)
            .await
            .ok_or_else(|| ApiError::unauthorized("invalid or expired token"))?;

        Ok(Self(user))
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::bad_request(error.to_string())
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
            store: Arc::new(AppStore::memory()),
        })
    }

    async fn auth_token(app: Router, name: &str, role: &str, ckb_address: &str) -> String {
        let connect_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/connect")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "ckb_address": ckb_address,
                            "wallet_type": "joyid_ckb",
                            "role": role,
                            "lock_script": {
                                "code_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                                "hash_type": "type",
                                "args": "0x1234"
                            },
                            "display_name": name
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(connect_response.status(), StatusCode::CREATED);
        let body = connect_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn signed_tx_fixture() -> serde_json::Value {
        json!({
            "hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
            "cellDeps": [],
            "headerDeps": [],
            "inputs": [{"previousOutput":{"txHash":"0x2222222222222222222222222222222222222222222222222222222222222222","index":"0x0"},"since":"0x0"}],
            "outputs": [{"capacity":"0x174876e800","lock":{"codeHash":"0x3333333333333333333333333333333333333333333333333333333333333333","hashType":"type","args":"0x1234"}}],
            "outputsData": ["0x"],
            "version": "0x0",
            "witnesses": ["0x55000000100000005500000055000000410000001111111122222222333333334444444455555555666666667777777788888888"]
        })
    }

    #[tokio::test]
    async fn protected_dashboard_requires_auth() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn lp_deposit_requires_signed_transaction_proof() {
        let app = test_app();
        let lp_token = auth_token(
            app.clone(),
            "Atlas LP",
            "lp",
            "ckt1qyq000000000000000000000000000000000000lp",
        )
        .await;

        let deposit_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deposits")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {lp_token}"))
                    .body(Body::from(
                        json!({"asset":"USDC","amount":5000}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(deposit_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lp_deposit_then_merchant_request_and_queue_fiber_channel() {
        let app = test_app();
        let lp_token = auth_token(
            app.clone(),
            "Atlas LP",
            "lp",
            "ckt1qyq000000000000000000000000000000000000lp",
        )
        .await;
        let merchant_token = auth_token(
            app.clone(),
            "Kairo Market",
            "merchant",
            "ckt1qyq0000000000000000000000000000000merchant",
        )
        .await;

        let deposit_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deposits")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {lp_token}"))
                    .body(Body::from(
                        json!({
                            "asset":"USDC",
                            "amount":5000,
                            "tx_hash":"0x1111111111111111111111111111111111111111111111111111111111111111",
                            "signed_tx": signed_tx_fixture()
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deposit_response.status(), StatusCode::CREATED);

        let request_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/liquidity/requests")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {merchant_token}"))
                    .body(Body::from(
                        json!({"asset":"USDC","amount":3000,"duration_days":30}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(request_response.status(), StatusCode::CREATED);
        let body = request_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let request_body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request_body["status"], "requested");
        let id = request_body["id"].as_str().unwrap();

        let deploy_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/liquidity/requests/{id}/deploy"))
                    .header(header::AUTHORIZATION, format!("Bearer {merchant_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deploy_response.status(), StatusCode::OK);
        let body = deploy_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let deployed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(deployed["status"], "pending_fiber_channel");
        assert!(deployed["channel_id"].is_null());
        assert!(
            deployed["fiber_note"]
                .as_str()
                .unwrap()
                .contains("Fiber RPC")
        );
    }
}

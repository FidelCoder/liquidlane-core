use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub struct Error(pub anyhow::Error);
macro_rules! ensure {
    ($condition:expr, $($message:tt)*) => {
        if !$condition { return Err(anyhow::anyhow!($($message)*).into()); }
    };
}
pub(crate) use ensure;
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        let status = if message.starts_with("unauthorized") {
            StatusCode::UNAUTHORIZED
        } else if message == "record not found" {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        tracing::warn!(error=%message,"marketplace request rejected");
        (status, Json(serde_json::json!({"error":message}))).into_response()
    }
}

use axum::{extract::FromRequestParts, http::request::Parts};

use super::{ApiError, AppState};
use crate::domain::User;

pub(crate) struct AuthedUser(pub(crate) User);

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

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode},
    Json,
};
use pos_core::{AuthError, Permission, SessionClaims};

use crate::AppState;

pub type ApiError = (StatusCode, Json<serde_json::Value>);

pub fn api_err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(serde_json::json!({ "error": msg.into() })))
}

pub fn auth_err_to_api(err: AuthError) -> ApiError {
    match err {
        AuthError::InvalidToken | AuthError::TokenExpired | AuthError::InvalidCredentials => {
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": err.to_string() })))
        }
        AuthError::AccountLocked(until) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": format!("Account locked until {until} due to failed PIN attempts"),
                "code": "ACCOUNT_LOCKED",
                "locked_until": until
            })),
        ),
        AuthError::Forbidden(perm) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!("Insufficient role permission: {perm:?} required"),
                "code": "FORBIDDEN"
            })),
        ),
        AuthError::ManagerOverrideRequired(action) => manager_override_err(&action),
    }
}

pub fn manager_override_err(action: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": format!("Manager or Owner PIN step-up approval required for: {action}"),
            "code": "MANAGER_OVERRIDE_REQUIRED",
            "required_action": action
        })),
    )
}

pub struct AuthenticatedSession(pub SessionClaims);

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedSession {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_val = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "Missing Authorization: Bearer <token> header",
                )
            })?;

        let token = header_val
            .strip_prefix("Bearer ")
            .or_else(|| header_val.strip_prefix("bearer "))
            .ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "Malformed Authorization header; expected 'Bearer <token>'",
                )
            })?;

        let claims = state
            .signer
            .verify_session(token.trim())
            .map_err(auth_err_to_api)?;

        Ok(AuthenticatedSession(claims))
    }
}

pub struct OptionalSession(pub Option<SessionClaims>);

#[async_trait]
impl FromRequestParts<AppState> for OptionalSession {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(header_val) = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        else {
            return Ok(OptionalSession(None));
        };

        let Some(token) = header_val
            .strip_prefix("Bearer ")
            .or_else(|| header_val.strip_prefix("bearer "))
        else {
            return Ok(OptionalSession(None));
        };

        let claims = state.signer.verify_session(token.trim()).ok();
        Ok(OptionalSession(claims))
    }
}

pub fn require_permission(claims: &SessionClaims, perm: Permission) -> Result<(), ApiError> {
    if claims.role.has_permission(perm) {
        Ok(())
    } else {
        Err(auth_err_to_api(AuthError::Forbidden(perm)))
    }
}

pub fn require_order_access(claims: &SessionClaims, order_id: &str) -> Result<(), ApiError> {
    if claims.can_access_order(order_id) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!("Session is not authorized to access order '{order_id}'"),
                "code": "ORDER_SCOPE_VIOLATION"
            })),
        ))
    }
}

pub fn require_manager_approval(
    state: &AppState,
    claims: &SessionClaims,
    headers: &HeaderMap,
    action_label: &str,
) -> Result<String, ApiError> {
    if claims.role.has_permission(Permission::ApproveManagerOverride) {
        return Ok(claims.actor_name.clone());
    }

    let Some(override_hdr) = headers
        .get("x-manager-override")
        .and_then(|v| v.to_str().ok())
    else {
        return Err(manager_override_err(action_label));
    };

    let mgr_claims = state
        .signer
        .verify_manager_override(override_hdr.trim())
        .map_err(|_| manager_override_err(action_label))?;

    Ok(format!("{} (Manager Override)", mgr_claims.manager_name))
}

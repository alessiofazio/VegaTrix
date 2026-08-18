use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use openpay_application::{ApiKeyRepository, UserRepository};
use openpay_crypto::{api_key_fingerprint, verify_secret};
use openpay_domain::{MerchantId, TenantId};
use openpay_persistence::PgStore;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub tenant_id: TenantId,
    pub merchant_id: Option<MerchantId>,
    pub actor_id: String,
    pub role: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    tenant_id: String,
    role: String,
    typ: String,
    exp: i64,
    iat: i64,
}

pub fn issue_tokens(
    secret_access: &str,
    secret_refresh: &str,
    user_id: &str,
    tenant_id: TenantId,
    role: &str,
) -> Result<(String, String), ApiError> {
    let now = OffsetDateTime::now_utc();
    let access = JwtClaims {
        sub: user_id.into(),
        tenant_id: tenant_id.as_prefixed(),
        role: role.into(),
        typ: "access".into(),
        iat: now.unix_timestamp(),
        exp: (now + Duration::minutes(15)).unix_timestamp(),
    };
    let refresh = JwtClaims {
        sub: user_id.into(),
        tenant_id: tenant_id.as_prefixed(),
        role: role.into(),
        typ: "refresh".into(),
        iat: now.unix_timestamp(),
        exp: (now + Duration::days(7)).unix_timestamp(),
    };
    let access_jwt = encode(
        &Header::default(),
        &access,
        &EncodingKey::from_secret(secret_access.as_bytes()),
    )
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth", "Auth error", "token"))?;
    let refresh_jwt = encode(
        &Header::default(),
        &refresh,
        &EncodingKey::from_secret(secret_refresh.as_bytes()),
    )
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "auth", "Auth error", "token"))?;
    Ok((access_jwt, refresh_jwt))
}

pub fn decode_refresh(secret: &str, token: &str) -> Result<JwtClaims, ApiError> {
    let data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "invalid token"))?;
    if data.claims.typ != "refresh" {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth",
            "Unauthorized",
            "wrong token type",
        ));
    }
    Ok(data.claims)
}

pub fn decode_access(secret: &str, token: &str) -> Result<JwtClaims, ApiError> {
    let data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "invalid token"))?;
    if data.claims.typ != "access" {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth",
            "Unauthorized",
            "wrong token type",
        ));
    }
    Ok(data.claims)
}

impl FromRequestParts<Arc<AppState>> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "missing bearer")
            })?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "bearer"))?;

        if token.starts_with("opk_") {
            return authenticate_api_key(&state.store, token).await;
        }

        let claims = decode_access(&state.config.jwt_access_secret, token)?;
        Ok(AuthContext {
            tenant_id: claims
                .tenant_id
                .parse()
                .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "tenant"))?,
            merchant_id: None,
            actor_id: claims.sub,
            role: claims.role,
            scopes: vec!["admin".into()],
        })
    }
}

async fn authenticate_api_key(store: &PgStore, token: &str) -> Result<AuthContext, ApiError> {
    let fingerprint = api_key_fingerprint(token);
    let record = store
        .find_by_fingerprint(&fingerprint)
        .await
        .map_err(openpay_application::ApplicationError::from)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "unknown key"))?;
    if record.revoked {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth",
            "Unauthorized",
            "revoked key",
        ));
    }
    if !verify_secret(token, &record.hash).unwrap_or(false) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth",
            "Unauthorized",
            "invalid key",
        ));
    }
    Ok(AuthContext {
        tenant_id: record.tenant_id,
        merchant_id: record.merchant_id,
        actor_id: record.id,
        role: "merchant".into(),
        scopes: record.scopes,
    })
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub edition: String,
    pub self_hosted: bool,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

pub async fn refresh(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::Json(body): axum::Json<RefreshRequest>,
) -> Result<axum::Json<TokenResponse>, ApiError> {
    let claims = decode_refresh(&state.config.jwt_refresh_secret, &body.refresh_token)?;
    let tenant_id: TenantId = claims.tenant_id.parse().map_err(|_| {
        ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "tenant")
    })?;
    let (access, refresh) = issue_tokens(
        &state.config.jwt_access_secret,
        &state.config.jwt_refresh_secret,
        &claims.sub,
        tenant_id,
        &claims.role,
    )?;
    Ok(axum::Json(TokenResponse {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".into(),
        expires_in: 900,
        edition: state.config.edition.as_str().into(),
        self_hosted: state.config.self_hosted,
    }))
}

pub async fn login(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::Json(body): axum::Json<LoginRequest>,
) -> Result<axum::Json<TokenResponse>, ApiError> {
    let user = state
        .store
        .find_by_email(&body.email)
        .await
        .map_err(openpay_application::ApplicationError::from)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "auth", "Unauthorized", "invalid credentials"))?;
    if !verify_secret(&body.password, &user.password_hash).unwrap_or(false) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "auth",
            "Unauthorized",
            "invalid credentials",
        ));
    }
    let (access, refresh) = issue_tokens(
        &state.config.jwt_access_secret,
        &state.config.jwt_refresh_secret,
        &user.id,
        user.tenant_id,
        &user.role,
    )?;
    Ok(axum::Json(TokenResponse {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".into(),
        expires_in: 900,
        edition: state.config.edition.as_str().into(),
        self_hosted: state.config.self_hosted,
    }))
}

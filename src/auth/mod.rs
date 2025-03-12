use axum::{
    http::{HeaderMap, StatusCode},
    response::{Response, IntoResponse},
    Json,
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};

// Simplified Supabase JWT claims structure with only essential fields
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub email: Option<String>,
    pub role: Option<String>,
    pub exp: usize,
    pub aud: Option<String>,
    pub iss: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// Simplified authentication function that accepts both JWT tokens and API keys
pub async fn verify_supabase_token(headers: &HeaderMap, jwt_secret: &str) -> Result<Claims, StatusCode> {
    // Check for Authorization header (Bearer token)
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_value) = auth_header.to_str() {
            if auth_value.starts_with("Bearer ") {
                let token = &auth_value[7..];
                return decode_and_validate_token(token, jwt_secret);
            }
        }
    }
    
    // Check for apikey header (for CLI compatibility)
    if let Some(api_key) = headers.get("apikey") {
        if let Ok(key_value) = api_key.to_str() {
            if !key_value.is_empty() {
                // Create a simplified anonymous user for API key authentication
                return Ok(Claims {
                    sub: "anon-user".to_string(),
                    email: None,
                    role: Some("anon".to_string()),
                    exp: usize::MAX,
                    aud: None,
                    iss: None,
                });
            }
        }
    }
    
    // No valid authentication found
    Err(StatusCode::UNAUTHORIZED)
}

// Simplified JWT token validation
fn decode_and_validate_token(token: &str, jwt_secret: &str) -> Result<Claims, StatusCode> {
    let key = DecodingKey::from_secret(jwt_secret.as_bytes());
    
    // Basic validation for Supabase JWTs
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.required_spec_claims.clear();

    decode::<Claims>(token, &key, &validation)
        .map(|token_data| token_data.claims)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

// Helper function for error responses
pub fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(ErrorResponse { error: message.to_string() })).into_response()
}

// Extract user ID from claims
pub fn extract_user_id(claims: &Claims) -> String {
    claims.sub.clone()
}

// Include middleware module
pub mod middleware;

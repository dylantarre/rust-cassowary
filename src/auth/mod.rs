use axum::{
    http::{HeaderMap, StatusCode},
    response::{Response, IntoResponse},
    Json,
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};

// Supabase JWT claims structure
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

// Authentication middleware for Supabase JWT verification
pub async fn verify_supabase_token(headers: &HeaderMap, jwt_secret: &str) -> Result<Claims, StatusCode> {
    let auth_header = headers
        .get("Authorization")
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_str()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = &auth_header[7..];
    let key = DecodingKey::from_secret(jwt_secret.as_bytes());
    
    // Configure validation for Supabase JWTs
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.required_spec_claims.clear(); // Don't require specific claims

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| {
            eprintln!("JWT decode error: {:?}", e);
            StatusCode::UNAUTHORIZED
        })?;

    Ok(token_data.claims)
}

// Helper function to create error responses
pub fn error_response(status: StatusCode, message: &str) -> Response {
    let body = Json(ErrorResponse {
        error: message.to_string(),
    });
    
    (status, body).into_response()
}

// Function to extract user ID from claims
pub fn extract_user_id(claims: &Claims) -> String {
    claims.sub.clone()
}

// Create a middleware module
pub mod middleware;

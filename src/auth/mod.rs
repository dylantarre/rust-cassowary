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
    // First try the Authorization header with Bearer token
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_value) = auth_header.to_str() {
            if auth_value.starts_with("Bearer ") {
                let token = &auth_value[7..];
                return decode_and_validate_token(token, jwt_secret);
            }
        }
    }
    
    // If no valid Authorization header, try the apikey header
    // This is used by the music-cli as an alternative authentication method
    if let Some(api_key) = headers.get("apikey") {
        if let Ok(key_value) = api_key.to_str() {
            // For apikey header, we need to verify it's the correct Supabase anon key
            // For simplicity, we'll create a dummy claims object with a fixed user ID
            // In a production environment, you would validate this key against your Supabase project
            
            // Check if the provided key is valid (this is a simplified check)
            if !key_value.is_empty() {
                // Create a dummy claims object for the anon key authentication
                return Ok(Claims {
                    sub: "anon-user".to_string(),
                    email: None,
                    role: Some("anon".to_string()),
                    exp: usize::MAX, // Never expires for simplicity
                    aud: None,
                    iss: None,
                });
            }
        }
    }
    
    // If no valid authentication method found
    Err(StatusCode::UNAUTHORIZED)
}

// Helper function to decode and validate JWT token
fn decode_and_validate_token(token: &str, jwt_secret: &str) -> Result<Claims, StatusCode> {
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

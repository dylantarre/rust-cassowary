use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::Response,
};

use super::{verify_supabase_token, error_response};
use crate::AppState;

// Simplified authentication middleware
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // Extract headers from the request
    let headers = req.headers();
    
    // Attempt to verify authentication
    match verify_supabase_token(headers, &state.supabase_jwt_secret).await {
        Ok(claims) => {
            // Store claims in request extensions and continue
            let mut req = req;
            req.extensions_mut().insert(claims);
            Ok(next.run(req).await)
        },
        Err(_) => {
            // Return a simple error response
            Err(error_response(
                axum::http::StatusCode::UNAUTHORIZED,
                "Authentication required",
            ))
        }
    }
}

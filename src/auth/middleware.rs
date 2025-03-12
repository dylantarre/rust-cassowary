use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::Response,
};

use super::{verify_supabase_token, error_response};
use crate::AppState;

// Middleware to enforce authentication on protected routes
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let headers = req.headers();
    
    match verify_supabase_token(headers, &state.supabase_jwt_secret).await {
        Ok(claims) => {
            // Store user claims in request extensions for later use
            let mut req = req;
            req.extensions_mut().insert(claims);
            
            // Continue to the handler
            Ok(next.run(req).await)
        },
        Err(status) => {
            Err(error_response(
                status,
                "Authentication required",
            ))
        }
    }
}

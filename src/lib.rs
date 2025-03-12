use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
    middleware,
};
use rand::seq::SliceRandom;
use serde::Deserialize;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

// Re-export the auth module
pub mod auth;
use auth::{middleware::require_auth, verify_supabase_token};

// Define the AppState struct
#[derive(Clone)]
pub struct AppState {
    pub music_dir: Arc<PathBuf>,
    pub supabase_jwt_secret: Arc<String>,
    pub track_ids: Vec<String>,
}

// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

// Get a random track and return a JSON response with the track ID
pub async fn random_track(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    // Try to authenticate with Supabase JWT or anon key
    // We'll allow this endpoint to work even without authentication
    let _ = verify_supabase_token(&headers, &state.supabase_jwt_secret).await;
    
    // Get all MP3 files from the music directory
    let mut track_ids = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&*state.music_dir) {
        for entry in entries.filter_map(Result::ok) {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".mp3") {
                    // Remove the .mp3 extension to get the track ID
                    if let Some(track_id) = file_name.strip_suffix(".mp3") {
                        track_ids.push(track_id.to_string());
                    }
                }
            }
        }
    }
    
    // Choose a random track
    let mut rng = rand::thread_rng();
    if let Some(track_id) = track_ids.choose(&mut rng) {
        // Instead of using a redirect, return a JSON response with the track ID
        // This is more compatible with the music-cli
        let json_response = Json(serde_json::json!({
            "track_id": track_id
        }));
        
        println!("Selected random track: {}", track_id);
        
        Ok(json_response)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// Stream a track by ID
pub async fn stream_track(
    Path(id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify the user is authenticated
    let _ = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Construct the file path
    let file_path = state.music_dir.join(format!("{}.mp3", id));
    
    // Open the file
    let file = match File::open(&file_path) {
        Ok(file) => file,
        Err(_) => return Err(StatusCode::NOT_FOUND),
    };
    
    // Get the file size
    let file_size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    
    // Create a response with the file content
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .body(axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(
            tokio::fs::File::open(file_path).await.map_err(|_| StatusCode::NOT_FOUND)?,
        )))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(response)
}

// Define the prefetch request structure
#[derive(Deserialize)]
pub struct PrefetchRequest {
    track_ids: Vec<String>,
}

// Prefetch tracks
pub async fn prefetch_tracks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<PrefetchRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify the user is authenticated
    let _ = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Check if all requested track IDs exist
    let mut valid_track_ids = Vec::new();
    let mut invalid_track_ids = Vec::new();
    
    for track_id in payload.track_ids {
        let file_path = state.music_dir.join(format!("{}.mp3", track_id));
        if file_path.exists() {
            valid_track_ids.push(track_id);
        } else {
            invalid_track_ids.push(track_id);
        }
    }
    
    // Return a response with the valid and invalid track IDs
    let response = Json(serde_json::json!({
        "valid_track_ids": valid_track_ids,
        "invalid_track_ids": invalid_track_ids,
    }));
    
    Ok(response)
}

// Get user info
pub async fn get_user_info(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify the user is authenticated and get the claims
    let claims = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Return the user info
    let response = Json(serde_json::json!({
        "user_id": claims.sub,
        "email": claims.email,
    }));
    
    Ok(response)
}

// Create the app with the given configuration
pub fn create_app(
    music_dir: Arc<PathBuf>,
    supabase_jwt_secret: Arc<String>,
) -> Router {
    // Create the app state
    let state = AppState {
        music_dir,
        supabase_jwt_secret,
        track_ids: Vec::new(),
    };
    
    // Set up CORS
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // Create a router for public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/random", get(random_track));

    // Create a router for protected routes (authentication required)
    let protected_routes = Router::new()
        .route("/tracks/:id", get(stream_track))
        .route("/prefetch", post(prefetch_tracks))
        .route("/user", get(get_user_info))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Combine the routes and add the state
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .with_state(state)
}

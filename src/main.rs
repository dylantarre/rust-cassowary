use axum::{
    extract::{Path as AxumPath, State, Json},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    collections::VecDeque,
    env,
    sync::Arc,
    net::SocketAddr,
};
use tokio::{
    io::AsyncReadExt,
    sync::Mutex,
};
use tokio_util::io::ReaderStream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use rand::seq::SliceRandom;
use regex::Regex;

// Import our auth module
mod auth;
use auth::{verify_supabase_token};

// Import from our library
use rusty_cassowary::create_app;

#[derive(Clone)]
struct AppState {
    music_dir: Arc<PathBuf>,
    supabase_jwt_secret: Arc<String>,
    prefetch_queue: Arc<Mutex<VecDeque<String>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrefetchRequest {
    track_ids: Vec<String>,
}

// Constant for the chunk size used in prefetching
const CHUNK_SIZE: usize = 8192; // 8KB chunks

// Health check endpoint
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

// Get a random track
async fn random_track(
    State(state): State<AppState>,
) -> impl IntoResponse {
    use std::fs;
    
    // Get all MP3 files in the music directory
    let mut track_ids = Vec::new();
    
    if let Ok(entries) = fs::read_dir(&*state.music_dir) {
        for entry in entries.filter_map(Result::ok) {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".mp3") {
                    // Remove the .mp3 extension
                    let track_id = file_name.trim_end_matches(".mp3").to_string();
                    track_ids.push(track_id);
                }
            }
        }
    }
    
    // Select a random track
    let mut rng = rand::thread_rng();
    if let Some(track_id) = track_ids.choose(&mut rng) {
        // Instead of using a redirect, return a JSON response with the track ID
        // This is more compatible with the music-cli
        println!("Selected random track: {}", track_id);
        
        Json(serde_json::json!({
            "track_id": track_id
        }))
    } else {
        Json(serde_json::json!({
            "error": "No tracks found"
        }))
    }
}

// Stream a track by ID
async fn stream_track(
    AxumPath(track_id): AxumPath<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let file_path = state.music_dir.join(format!("{}.mp3", track_id));
    
    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Verify authentication
    verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Open the file
    let file = match tokio::fs::File::open(&file_path).await {
        Ok(file) => file,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    
    // Get the file size
    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    
    let file_size = metadata.len();
    
    // Check if the client requested a range
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| {
            let caps = Regex::new(r"bytes=(\d+)-(\d+)?")
                .unwrap()
                .captures(s)?;
            let start = caps.get(1)?.as_str().parse::<u64>().ok()?;
            let end = caps.get(2)
                .and_then(|m| m.as_str().parse::<u64>().ok())
                .unwrap_or(file_size - 1);
            Some((start, end))
        });
    
    match range {
        Some((start, end)) => {
            // Create a limited reader for the range
            let stream = ReaderStream::new(file.take(end - start + 1));
            
            let body = axum::body::Body::from_stream(stream);
            let mut response = Response::new(body);
            
            // Set the content type
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("audio/mpeg"),
            );
            
            // Set the content range header
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size)).unwrap(),
            );
            
            // Set the content length header
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&format!("{}", end - start + 1)).unwrap(),
            );
            
            response.status_mut().clone_from(&StatusCode::PARTIAL_CONTENT);
            
            Ok(response)
        }
        None => {
            // Stream the entire file
            let stream = ReaderStream::new(file);
            
            let body = axum::body::Body::from_stream(stream);
            let mut response = Response::new(body);
            
            // Set the content type
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("audio/mpeg"),
            );
            
            // Set the content length header
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&format!("{}", file_size)).unwrap(),
            );
            
            Ok(response)
        }
    }
}

// Prefetch tracks
async fn prefetch_tracks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PrefetchRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify authentication
    verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Add the track IDs to the prefetch queue
    for track_id in request.track_ids {
        state.prefetch_queue.lock().await.push_back(track_id);
    }
    
    Ok(StatusCode::OK)
}

// Get user info
async fn user_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let claims = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    #[derive(Serialize)]
    struct UserInfo {
        id: String,
        email: Option<String>,
        role: String,
    }
    
    let user_info = UserInfo {
        id: claims.sub,
        email: claims.email,
        role: claims.role.unwrap_or_else(|| "authenticated".to_string()),
    };
    
    Ok(Json(user_info))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    // Load environment variables from .env file
    dotenv().ok();
    
    // Get the music directory from the environment
    let music_dir = env::var("MUSIC_DIR")
        .expect("MUSIC_DIR must be set");
    
    // Get the Supabase JWT secret from the environment
    let supabase_jwt_secret = env::var("SUPABASE_JWT_SECRET")
        .expect("SUPABASE_JWT_SECRET must be set");
    
    // Get the port from the environment or use the default
    let port = env::var("PORT")
        .unwrap_or_else(|_| "3500".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");
    
    // Create the app with the given configuration
    let app = create_app(
        Arc::new(PathBuf::from(music_dir)),
        Arc::new(supabase_jwt_secret),
    );
    
    // Start the server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Music streaming server listening on {}", addr);
    
    // Use tokio::net::TcpListener for binding
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind server");
        
    // Run the server with axum
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
    }
}

// Background task to prefetch tracks
async fn prefetch_task(state: AppState) {
    loop {
        // Get the next track ID from the queue
        let track_id = {
            let mut queue = state.prefetch_queue.lock().await;
            queue.pop_front()
        };

        if let Some(track_id) = track_id {
            let file_path = state.music_dir.join(format!("{}.mp3", track_id));
            if file_path.exists() {
                // Pre-read the file into memory cache
                // This relies on the OS's file system cache
                if let Ok(mut file) = File::open(&file_path) {
                    let mut buffer = [0; CHUNK_SIZE];
                    while let Ok(n) = file.read(&mut buffer) {
                        if n == 0 { break; }
                    }
                    tracing::info!("Prefetched track: {}", track_id);
                }
            }
        }
        
        // Wait a bit before checking the queue again
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

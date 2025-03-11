mod schema;

use axum::{
    extract::{Path, State, Query},
    http::{header, HeaderMap, HeaderValue, StatusCode, Range},
    response::{IntoResponse, Response, sse::{Event, Sse}},
    routing::{get, post},
    Router,
    Json,
};
use bytes::{Bytes, BytesMut};
use dotenv::dotenv;
use futures::{Stream, StreamExt};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    net::SocketAddr,
    path::{Path as FilePath, PathBuf},
    sync::Arc,
    collections::VecDeque,
};
use tokio::{
    fs::File as TokioFile,
    io::AsyncReadExt,
    sync::Mutex,
};
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;
use tracing::{info, error, Level};
use walkdir::WalkDir;

const CHUNK_SIZE: usize = 1024 * 32; // 32KB chunks

#[derive(Clone)]
struct AppState {
    music_dir: Arc<PathBuf>,
    supabase_jwt_secret: Arc<String>,
    prefetch_queue: Arc<Mutex<VecDeque<String>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrefetchRequest {
    track_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

async fn verify_token(headers: &HeaderMap, jwt_secret: &str) -> Result<String, StatusCode> {
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
    let validation = Validation::default();

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(token_data.claims.sub)
}

async fn stream_track(
    Path(track_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let _user_id = verify_token(&headers, &state.supabase_jwt_secret).await?;
    
    let file_path = state.music_dir.join(format!("{}.mp3", track_id));
    
    if !file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let file_size = file
        .metadata()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .len();

    // Parse Range header
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| {
            let caps = s.strip_prefix("bytes=")?;
            let mut parts = caps.split('-');
            let start = parts.next()?.parse::<u64>().ok()?;
            let end = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(file_size - 1);
            Some((start, end))
        });

    match range {
        Some((start, end)) => {
            // Create a limited reader for the range
            let stream = ReaderStream::new(file.take(end - start + 1));
            
            let mut response = Response::new(axum::body::StreamBody::new(stream));
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, file_size))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            );
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("audio/mpeg"),
            );
            response.headers_mut().insert(
                header::ACCEPT_RANGES,
                HeaderValue::from_static("bytes"),
            );
            *response.status_mut() = StatusCode::PARTIAL_CONTENT;

            Ok(response)
        }
        None => {
            // Stream the entire file
            let stream = ReaderStream::new(file);
            
            let mut response = Response::new(axum::body::StreamBody::new(stream));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("audio/mpeg"),
            );
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&file_size.to_string())
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            );
            response.headers_mut().insert(
                header::ACCEPT_RANGES,
                HeaderValue::from_static("bytes"),
            );

            Ok(response)
        }
    }
}

async fn prefetch_tracks(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PrefetchRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let _user_id = verify_token(&headers, &state.supabase_jwt_secret).await?;
    
    // Add tracks to prefetch queue
    let mut queue = state.prefetch_queue.lock().await;
    for track_id in request.track_ids {
        queue.push_back(track_id);
    }

    Ok(StatusCode::OK)
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let music_dir = Arc::new(PathBuf::from(
        std::env::var("MUSIC_DIR").unwrap_or_else(|_| "./music".to_string())
    ));
    
    let supabase_jwt_secret = Arc::new(
        std::env::var("SUPABASE_JWT_SECRET").expect("SUPABASE_JWT_SECRET must be set"),
    );

    let state = AppState {
        music_dir,
        supabase_jwt_secret,
        prefetch_queue: Arc::new(Mutex::new(VecDeque::new())),
    };

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/tracks/:id", get(stream_track))
        .route("/prefetch", post(prefetch_tracks))
        .route("/health", get(health_check))
        .layer(cors)
        .with_state(state.clone());

    // Start the prefetch worker
    tokio::spawn(prefetch_worker(state));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);
        
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Music streaming server listening on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn prefetch_worker(state: AppState) {
    loop {
        // Check the prefetch queue
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
                        // Just reading is enough to cache
                    }
                }
            }
        }

        // Sleep briefly to prevent busy-waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

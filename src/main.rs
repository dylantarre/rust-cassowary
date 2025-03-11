use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
    Json,
    middleware,
};
use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read},
    net::SocketAddr,
    path::{PathBuf},
    sync::Arc,
    collections::VecDeque,
};
use tokio::{
    io::AsyncReadExt,
    sync::Mutex,
    net::TcpListener,
};
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;
use tracing::{info, Level};

// Import our auth module
mod auth;
use auth::{verify_supabase_token, Claims};
use auth::middleware::require_auth;

const CHUNK_SIZE: usize = 1024 * 32; // 32KB chunks

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

async fn stream_track(
    Path(track_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // Get user claims from the request extensions (set by middleware)
    let _claims = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
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
            
            let body = axum::body::Body::from_stream(stream);
            let mut response = Response::new(body);
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
            
            let body = axum::body::Body::from_stream(stream);
            let mut response = Response::new(body);
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
    // Get user claims from the request extensions (set by middleware)
    let _claims = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
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

// New endpoint to get user info from the token
async fn get_user_info(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let claims = verify_supabase_token(&headers, &state.supabase_jwt_secret).await?;
    
    #[derive(Serialize)]
    struct UserInfo {
        id: String,
        email: Option<String>,
        role: Option<String>,
    }
    
    let user_info = UserInfo {
        id: claims.sub,
        email: claims.email,
        role: claims.role,
    };
    
    Ok(Json(user_info))
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

    // Create a router for public routes
    let public_routes = Router::new()
        .route("/health", get(health_check));

    // Create a router for protected routes
    let protected_routes = Router::new()
        .route("/tracks/:id", get(stream_track))
        .route("/prefetch", post(prefetch_tracks))
        .route("/user", get(get_user_info))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Combine the routers
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
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
    
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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

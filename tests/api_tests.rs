use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use dotenv::dotenv;
use serde_json::Value;
use std::fs;
use std::sync::Arc;
use std::path::PathBuf;
use tempfile::tempdir;
use tower::ServiceExt;

// Import the necessary modules from the main crate
use rusty_cassowary::create_app;

// Helper function to create a test app with a temporary music directory
async fn setup_test_app() -> (Router, tempfile::TempDir) {
    dotenv().ok();
    
    // Create a temporary directory for test music files
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let temp_path = temp_dir.path().to_str().unwrap().to_string();
    
    // Create some test MP3 files
    let test_files = vec!["test1.mp3", "test2.mp3", "test3.mp3"];
    for file in &test_files {
        let file_path = temp_dir.path().join(file);
        fs::write(&file_path, "test mp3 data").expect("Failed to write test file");
    }
    
    // Create the app with the test configuration
    let supabase_jwt_secret = std::env::var("SUPABASE_JWT_SECRET")
        .unwrap_or_else(|_| "test_jwt_secret".to_string());
    
    let app = create_app(
        Arc::new(PathBuf::from(temp_path)),
        Arc::new(supabase_jwt_secret),
    );
    
    (app, temp_dir)
}

#[tokio::test]
async fn test_health_endpoint() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create a request to the health endpoint
    let response = app
        .oneshot(Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    // Check that we get a 200 OK response
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_random_endpoint() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create a request to the random endpoint
    let response = app
        .oneshot(Request::builder()
            .uri("/random")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    // Check that we get a 200 OK response
    assert_eq!(response.status(), StatusCode::OK);
    
    // Extract the response body
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    
    // Parse the JSON response
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    
    // Check that the response contains a track_id field
    assert!(json.get("track_id").is_some());
    
    // Check that the track_id is one of our test files (without the .mp3 extension)
    let track_id = json.get("track_id").unwrap().as_str().unwrap();
    assert!(track_id == "test1" || track_id == "test2" || track_id == "test3");
}

#[tokio::test]
async fn test_tracks_endpoint_with_auth() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create a request to the tracks endpoint with a valid track ID
    let response = app.clone()
        .oneshot(Request::builder()
            .uri("/tracks/test1")
            .header("Authorization", "Bearer test_token")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    // Since we're using a test token that won't validate, we should get a 401 Unauthorized
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_tracks_endpoint_with_apikey() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Try with the apikey header
    let response = app
        .oneshot(Request::builder()
            .uri("/tracks/test1")
            .header("apikey", "test_anon_key")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    // This should actually succeed with 200 since we're using a test key
    assert_eq!(response.status(), StatusCode::OK);
}

// Test with invalid authentication
#[tokio::test]
async fn test_invalid_auth() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create a request to a protected endpoint with an invalid token
    let response = app
        .oneshot(Request::builder()
            .uri("/tracks/test1")
            .header("Authorization", "Bearer invalid_token")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    
    // Check that we get a 401 Unauthorized response
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// Test the prefetch endpoint
#[tokio::test]
async fn test_prefetch_endpoint() {
    // Setup the test app
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create a request to the prefetch endpoint
    let response = app
        .oneshot(Request::builder()
            .uri("/prefetch")
            .method("POST")
            .header("content-type", "application/json")
            .header("Authorization", "Bearer test_token")
            .body(Body::from(r#"{"track_ids": ["test1", "test2"]}"#))
            .unwrap())
        .await
        .unwrap();
    
    // Since we're using a test token that won't validate, we should get a 401 Unauthorized
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

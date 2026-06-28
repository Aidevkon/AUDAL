use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_get_flavours_router_wiring() {
    let audit_dir = TempDir::new().unwrap();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap());
    
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit).await;
    let app = m0d::build_router_for_test(state);

    let request = Request::builder()
        .method("GET")
        .uri("/mix/flavours")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert!(json.get("active").is_some(), "Missing active field in response");
    assert!(json.get("flavours").is_some(), "Missing flavours field in response");
    
    let flavours_array = json.get("flavours").unwrap().as_array().unwrap();
    assert!(!flavours_array.is_empty(), "Flavours array should not be empty");
}

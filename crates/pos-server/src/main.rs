use std::net::SocketAddr;

use pos_server::{build_router, AppState};
use pos_storage::PosStorage;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pos_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_dir = std::env::var("POS_DATA_DIR").unwrap_or_else(|_| "./pos_data".to_string());
    let port: u16 = std::env::var("POS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4000);

    let storage = PosStorage::open_or_init(&data_dir);
    let state = AppState::new(storage);
    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("POS Core Edge Server listening on http://127.0.0.1:{port}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind POS server listener");
    axum::serve(listener, app)
        .await
        .expect("POS server terminated unexpectedly");
}

use serde::Serialize;

#[derive(Serialize)]
pub struct EdgeRuntimeInfo {
    pub version: &'static str,
    pub mode: &'static str,
    pub local_hub_url: &'static str,
}

#[tauri::command]
fn pos_edge_info() -> EdgeRuntimeInfo {
    EdgeRuntimeInfo {
        version: "0.2.0",
        mode: if cfg!(mobile) {
            "mobile_handheld_or_kiosk"
        } else {
            "desktop_embedded_edge_hub"
        },
        local_hub_url: "http://127.0.0.1:4000",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // On Linux & Windows Desktop terminals, boot the embedded Rust Edge Hub (`pos-server` + `redb`) automatically
    #[cfg(not(mobile))]
    std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create embedded POS Tokio runtime");

        rt.block_on(async {
            let data_dir =
                std::env::var("POS_DATA_DIR").unwrap_or_else(|_| "./pos_data".to_string());
            let storage = pos_storage::PosStorage::open_or_init(data_dir);
            let app = pos_server::build_router(pos_server::AppState::new(storage));
            if let Ok(listener) = tokio::net::TcpListener::bind("0.0.0.0:4000").await {
                let _ = axum::serve(listener, app).await;
            }
        });
    });

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![pos_edge_info])
        .run(tauri::generate_context!())
        .expect("error while running OmniPOS Tauri v2 application");
}

//! Tauri desktop-shell entrypoint (only under `tauri-app`).
//!
//! Composition root: builds one [`HandoffState`], one router over it, and
//! hands both to the IPC layer. Live mesh messages are forwarded to the
//! frontend as `ramesh://message` events so the feed updates without
//! polling. The same router is also served on loopback HTTP for headless
//! local agents — same state, same gate, same feed.

#![cfg(feature = "tauri-app")]

use tauri::Emitter;

use crate::commands::{self, IpcState};
use crate::mesh::MeshEventKind;
use crate::server::{handoff_router, serve_loopback, HandoffState};

/// Loopback port for headless local agents (mnemonic: x402).
pub const LOOPBACK_PORT: u16 = 3402;

/// Run the Hammurabi Handoff shell.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Composition root for the settlement backend: mock unless `x402-live`
    // is compiled in *and* every `HANDOFF_X402_*` env var is set. Enabling
    // the feature flag alone must never be enough to go live — see
    // `settlement_backend::from_env_or_default`.
    let backend = crate::settlement_backend::from_env_or_default();
    let backend_label = backend.label();
    let state = HandoffState::with_backend(backend);
    let router = handoff_router(state.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .manage(IpcState {
            state: state.clone(),
            router,
        })
        .setup(move |tauri_app| {
            // Live feed → frontend events. A lagged receiver must NOT kill
            // the forwarder — skip ahead and keep streaming (the frontend
            // can always re-sync the gap via `mesh_feed`).
            let mut live = state.mesh.subscribe();
            let handle = tauri_app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tokio::sync::broadcast::error::RecvError;
                loop {
                    match live.recv().await {
                        Ok(message) => {
                            let _ = handle.emit("ramesh://message", &message);
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!("mesh forwarder lagged; skipped {skipped} messages");
                        }
                        Err(RecvError::Closed) => break,
                    }
                }
            });

            // Loopback HTTP surface for headless local agents. Same state,
            // same router construction — one gate, one feed.
            let http_state = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = serve_loopback(http_state, LOOPBACK_PORT).await {
                    tracing::warn!("loopback surface unavailable: {e}");
                }
            });

            state.mesh.post(
                "system/handoff",
                MeshEventKind::Info,
                &format!(
                    "Hammurabi Handoff online — RAMesh live, x402 gate armed ({backend_label}), loopback :{LOOPBACK_PORT}."
                ),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::mesh_feed,
            commands::agent_announce,
            commands::x402_settle,
            commands::request_execution,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

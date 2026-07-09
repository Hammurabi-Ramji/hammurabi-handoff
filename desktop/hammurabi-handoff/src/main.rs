//! Hammurabi Handoff desktop shell (builds only with `--features tauri-app`).

fn main() {
    tracing_subscriber_init();
    if let Err(e) = hammurabi_handoff::app::run() {
        eprintln!("Hammurabi Handoff failed to start: {e}");
        std::process::exit(1);
    }
}

/// Local stderr tracing only — zero telemetry, nothing leaves the machine.
fn tracing_subscriber_init() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}

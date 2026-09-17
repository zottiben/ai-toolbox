//! The board, as a desktop app.
//!
//! Tauri rather than Electron, for a reason that is structural rather than aesthetic: the
//! host process here *is* Rust, so it links `ai-toolbox-core` and `ai-toolbox-ui`
//! directly and there is no IPC boundary, no second runtime, and no second copy of the
//! merge rules in another language. It also uses the platform webview instead of shipping
//! Chromium, which is the difference between an app measured in tens of megabytes and one
//! measured in hundreds.
//!
//! The window is deliberately thin. It starts the same server `ai-toolbox ui` starts and
//! points a webview at it, so there is one frontend, one API and one set of tests - the
//! desktop app cannot drift from the browser app because it *is* the browser app.

// No console window behind the app on Windows. Only in release: a debug build's stdout is
// how you find out why it did not start.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::mpsc;

use anyhow::{Context, Result};
use tauri::{WebviewUrl, WebviewWindowBuilder};

use ai_toolbox_core::registry::Registry;
use ai_toolbox_core::root;
use ai_toolbox_ui::{ServeOptions, Server};

fn main() {
    if let Err(err) = run() {
        // A desktop app has nowhere to print, so a startup failure gets a dialog. The
        // commonest one by far is a missing catalogue, which has a one-line fix.
        eprintln!("ai-toolbox: {err:#}");
        alert(&format!("{err:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let catalogue_root = root::find().context(
        "could not find the ai-toolbox catalogue - reinstall, or set AI_TOOLBOX to a clone",
    )?;
    let registry = Registry::open().context("opening the project registry")?;

    // The server needs a runtime that outlives this function, and Tauri owns the main
    // thread for the event loop, so the runtime is leaked deliberately rather than
    // dropped at the end of `run` and taking the server with it.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    let runtime = Box::leak(Box::new(runtime));

    let (ready, started) = mpsc::channel();
    runtime.spawn(async move {
        match Server::bind(catalogue_root, registry, ServeOptions::default()).await {
            Ok(server) => {
                let _ = ready.send(Ok(server.url()));
                let _ = server.serve().await;
            }
            Err(err) => {
                let _ = ready.send(Err(err.to_string()));
            }
        }
    });

    let url = started
        .recv()
        .context("the board server did not start")?
        .map_err(|e| anyhow::anyhow!(e))?;
    eprintln!("ai-toolbox: serving {url}");
    let url = url.parse().context("the board produced an unusable URL")?;

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(move |app| {
            // Built here rather than in tauri.conf.json because the URL is not known
            // until the OS has assigned a port and the token has been minted.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("ai-toolbox")
                .inner_size(1280.0, 860.0)
                .min_inner_size(900.0, 520.0)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("running the desktop app")
}

/// Tauri's dialog plugin is not loaded yet when startup fails, so this uses the platform's
/// own facility and falls back to stderr where there is not one.
fn alert(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display dialog {} with title \"ai-toolbox\" buttons {{\"OK\"}} with icon caution",
            applescript_string(message)
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = message;
    }
}

#[cfg(target_os = "macos")]
fn applescript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

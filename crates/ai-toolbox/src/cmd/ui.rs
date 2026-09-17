//! `ai-toolbox ui` - the board.

use ai_toolbox_core::registry::Registry;
use ai_toolbox_ui::{ServeOptions, Server};

use crate::out;

pub fn run(catalogue_root: std::path::PathBuf, port: u16, open: bool) -> anyhow::Result<()> {
    let registry = Registry::open()?;

    // Its own runtime rather than `#[tokio::main]`, so the rest of the CLI stays
    // synchronous and pays nothing for a command it does not use.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let server = Server::bind(catalogue_root, registry, ServeOptions { port, token: None })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let url = server.url();
        let bundle = ai_toolbox_ui::bundle();
        if !bundle.embedded {
            out::warn("this binary has no frontend compiled in - the board will explain itself.");
        }
        out::ok(&format!("board at {url}"));
        out::info("loopback only, and the token is minted per run - closing this stops it.");

        if open {
            launch(&url);
        }
        out::info("Ctrl-C to stop.");
        server.serve().await.map_err(|e| anyhow::anyhow!("{e}"))
    })?;
    Ok(())
}

/// Open the browser, and say so rather than failing if there is nothing to open with -
/// the URL is already printed, so a headless machine loses nothing.
fn launch(url: &str) {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let opener = "";

    if opener.is_empty() {
        return;
    }
    let launched = std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !launched {
        out::info("could not open a browser - the URL above is the whole of it.");
    }
}

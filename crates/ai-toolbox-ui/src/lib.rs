//! The board: a local server over the same engine the CLI uses.
//!
//! Rust on `ai-toolbox-core` rather than a second service in another language (D1). The
//! merge rules, the classification and the repair logic are already there and are careful
//! about things a reimplementation would get wrong - so every handler calls the core, and
//! none of them decides anything itself.
//!
//! The server binds loopback only and mints a token per run. That is deliberate and not
//! configurable: this API installs files and repairs repos anywhere on the machine, which
//! is not something to expose by flag.

mod assets;
mod auth;
mod error;
mod events;
mod read;
mod state;
mod write;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::Router;
use tokio::net::TcpListener;

use ai_toolbox_core::registry::Registry;

pub use auth::{TOKEN_HEADER, TOKEN_QUERY};
pub use error::{Error, Result};
pub use state::AppState;
pub use write::Request;

/// What the binary was built with. Worth being able to ask: a binary compiled without a
/// frontend serves an explanation instead of a board.
pub fn bundle() -> Bundle {
    Bundle {
        embedded: assets::is_embedded(),
        files: assets::len(),
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Bundle {
    pub embedded: bool,
    pub files: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// 0 asks the OS for a free one, so two boards never fight over a number.
    pub port: u16,
    /// Supply one to keep a URL stable across restarts. Otherwise it is minted fresh.
    pub token: Option<String>,
}

/// Bound, but not yet serving. Split in two so the CLI can print and open the real URL -
/// which it cannot know until the OS has assigned the port - before it blocks.
pub struct Server {
    addr: SocketAddr,
    token: String,
    listener: TcpListener,
    router: Router,
}

impl Server {
    pub async fn bind(
        catalogue_root: PathBuf,
        registry: Registry,
        options: ServeOptions,
    ) -> Result<Server> {
        let token = match options.token {
            Some(t) if !t.trim().is_empty() => t,
            _ => auth::mint_token()?,
        };
        let state = AppState::new(catalogue_root, registry, token.as_str());

        let api = read::routes()
            .merge(write::routes())
            .merge(events::routes())
            .route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::require_token,
            ));

        events::watch(state.clone(), state.changes());

        let router = Router::new()
            .nest("/api", api)
            .fallback(assets::serve)
            .with_state(state);

        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, options.port)))
            .await
            .map_err(|e| Error::BadRequest(format!("binding 127.0.0.1:{}: {e}", options.port)))?;
        let addr = listener
            .local_addr()
            .map_err(|e| Error::BadRequest(e.to_string()))?;

        Ok(Server {
            addr,
            token,
            listener,
            router,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// The URL to open. The token rides in the query because it is the only channel a
    /// freshly opened browser tab has.
    pub fn url(&self) -> String {
        format!("http://{}/?{}={}", self.addr, TOKEN_QUERY, self.token)
    }

    pub async fn serve(self) -> Result<()> {
        axum::serve(self.listener, self.router)
            .await
            .map_err(|e| Error::BadRequest(e.to_string()))?;
        Ok(())
    }
}

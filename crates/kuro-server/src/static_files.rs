//! Serving the built web interface.
//!
//! The frontend is a separate Vite build. In development it runs on its own
//! port and talks to this server over CORS; in normal use the built assets in
//! `web/dist` are served from the same origin so there is one process and one
//! URL.

use std::path::PathBuf;

use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::SharedState;

pub fn router() -> Router<SharedState> {
    match web_dir() {
        Some(dir) => {
            // Unknown paths fall back to index.html so client-side routes such
            // as /models survive a page reload.
            let index = ServeFile::new(dir.join("index.html"));
            Router::new().fallback_service(ServeDir::new(dir).fallback(index))
        }
        None => Router::new().fallback(missing_frontend),
    }
}

/// Locate the built frontend.
///
/// `KURO_WEB_DIR` wins, so a packaged build can point somewhere else without
/// recompiling.
fn web_dir() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("KURO_WEB_DIR") {
        let path = PathBuf::from(configured);
        return path.join("index.html").exists().then_some(path);
    }

    // Alongside the running binary when packaged, and up from the workspace
    // target directory when run with `cargo run`.
    let candidates = [
        PathBuf::from("web/dist"),
        PathBuf::from("../web/dist"),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("web")))
            .unwrap_or_default(),
    ];

    candidates
        .into_iter()
        .find(|path| path.join("index.html").exists())
}

/// Shown when the API is up but the interface has not been built.
///
/// This is a normal state during development, so it explains the fix rather
/// than looking like a failure.
async fn missing_frontend() -> Response {
    let page = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Kuro LLM</title>
  <style>
    :root { color-scheme: light dark; }
    body {
      margin: 0; min-height: 100vh;
      display: grid; place-items: center;
      font: 15px/1.6 ui-sans-serif, -apple-system, system-ui, sans-serif;
      background: #0a0a0a; color: #ededed;
    }
    main { max-width: 34rem; padding: 2rem; }
    h1 { font-size: 1.25rem; font-weight: 600; margin: 0 0 .5rem; letter-spacing: -0.01em; }
    p { color: #a1a1a1; margin: 0 0 1rem; }
    code {
      display: block; padding: .75rem 1rem; margin: .25rem 0;
      background: #171717; border: 1px solid #262626; border-radius: .5rem;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px;
      color: #ededed;
    }
    a { color: #ededed; }
  </style>
</head>
<body>
  <main>
    <h1>Kuro LLM is running</h1>
    <p>The API is up, but the web interface has not been built yet. From the project directory:</p>
    <code>cd web &amp;&amp; npm install &amp;&amp; npm run build</code>
    <p>Then reload this page. The API is available now at
       <a href="/api/status">/api/status</a> and <a href="/api/models">/api/models</a>.</p>
  </main>
</body>
</html>"#;

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(page),
    )
        .into_response()
}

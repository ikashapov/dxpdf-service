//! HTTP layer: axum router exposing dxpdf conversion.
//!
//! `POST /convert?image-dpi=<n>` with the raw DOCX bytes as the request body
//! returns the rendered PDF bytes. `GET /health` is a liveness probe.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::Semaphore;

/// Mirrors the dxpdf CLI bounds: values outside catch typos, the library
/// itself clamps the floor.
const MIN_IMAGE_DPI: f32 = 1.0;
const MAX_IMAGE_DPI: f32 = 2400.0;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_body_mb: usize,
    pub concurrency: usize,
}

struct AppState {
    /// Caps how many CPU-bound conversions run at once; excess requests queue.
    convert_slots: Semaphore,
}

pub fn build_router(config: &ServerConfig) -> Router {
    let state = Arc::new(AppState {
        convert_slots: Semaphore::new(config.concurrency.max(1)),
    });
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/convert", post(convert))
        .layer(DefaultBodyLimit::max(config.max_body_mb * 1024 * 1024))
        .with_state(state)
}

/// Runs the server until `shutdown` resolves (SCM stop / Ctrl+C).
pub async fn serve(
    config: ServerConfig,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{e}")))?;
    let app = build_router(&config);
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("failed to bind {addr} (is the port already in use?): {e}"),
        )
    })?;
    log::info!(
        "listening on http://{addr} (max body {} MB)",
        config.max_body_mb
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

async fn convert(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let image_dpi = match parse_image_dpi(&params) {
        Ok(dpi) => dpi,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "request body is empty; send the DOCX file bytes as the body".to_string(),
        )
            .into_response();
    }

    // Never rejects: the semaphore is not closed while the app runs.
    let _slot = state.convert_slots.acquire().await;

    let started = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        let options = dxpdf::RenderOptions::default().with_image_dpi(image_dpi);
        dxpdf::convert_with_options(&body, &options)
    })
    .await;

    match result {
        Ok(Ok(pdf)) => {
            log::info!(
                "converted {} DPI in {:?} -> {} bytes",
                image_dpi,
                started.elapsed(),
                pdf.len()
            );
            (
                [
                    (header::CONTENT_TYPE, "application/pdf"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"output.pdf\"",
                    ),
                ],
                pdf,
            )
                .into_response()
        }
        Ok(Err(e)) => {
            log::warn!("conversion failed: {e}");
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("conversion failed: {e}"),
            )
                .into_response()
        }
        Err(e) => {
            log::error!("conversion task panicked: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error during conversion".to_string(),
            )
                .into_response()
        }
    }
}

/// Parses `image-dpi` (also accepting `image_dpi`), defaulting to
/// [`dxpdf::DEFAULT_IMAGE_DPI`]. Out-of-range and non-numeric values are
/// rejected with a clear message instead of silently clamped, mirroring the
/// dxpdf CLI.
fn parse_image_dpi(params: &HashMap<String, String>) -> Result<f32, String> {
    let raw = match params.get("image-dpi").or_else(|| params.get("image_dpi")) {
        Some(s) => s,
        None => return Ok(dxpdf::DEFAULT_IMAGE_DPI),
    };
    let dpi: f32 = raw
        .parse()
        .map_err(|_| format!("image-dpi: `{raw}` is not a valid number"))?;
    if !dpi.is_finite() || !(MIN_IMAGE_DPI..=MAX_IMAGE_DPI).contains(&dpi) {
        return Err(format!(
            "image-dpi: must be between {MIN_IMAGE_DPI} and {MAX_IMAGE_DPI} (got `{raw}`)"
        ));
    }
    Ok(dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(v: &[(&str, &str)]) -> HashMap<String, String> {
        v.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn image_dpi_defaults_when_absent() {
        assert_eq!(
            parse_image_dpi(&params(&[])).unwrap(),
            dxpdf::DEFAULT_IMAGE_DPI
        );
    }

    #[test]
    fn image_dpi_accepts_both_spellings_and_range() {
        assert_eq!(
            parse_image_dpi(&params(&[("image-dpi", "300")])).unwrap(),
            300.0
        );
        assert_eq!(
            parse_image_dpi(&params(&[("image_dpi", "72")])).unwrap(),
            72.0
        );
        assert_eq!(
            parse_image_dpi(&params(&[("image-dpi", "1")])).unwrap(),
            1.0
        );
        assert_eq!(
            parse_image_dpi(&params(&[("image-dpi", "2400")])).unwrap(),
            2400.0
        );
    }

    #[test]
    fn image_dpi_rejects_bad_values() {
        for bad in ["0", "-300", "2401", "nan", "inf", "abc", ""] {
            assert!(
                parse_image_dpi(&params(&[("image-dpi", bad)])).is_err(),
                "`{bad}` should be rejected"
            );
        }
    }
}

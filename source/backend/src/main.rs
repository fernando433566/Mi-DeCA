//! DeCA Server: backend para generar Documentos electrónicos de Control
//! Administrativo (DeCA) según la Orden FOM/2861/2012 y la Resolución de la
//! DGTCF de 5 de junio de 2026.

mod app_update;
mod auth;
mod backup;
mod config;
mod db;
mod deca;
mod error;
mod jobs;
mod maps;
mod preferences;
mod public_url;
mod routes;
mod state;

use std::sync::Arc;

use anyhow::Context;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")))
        .init();

    let config = config::Config::from_env();
    for dir in [config.data_dir.clone(), config.deca_dir(), config.tiles_dir(), config.tmp_dir()] {
        std::fs::create_dir_all(&dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    }

    let pool = db::connect(&config.db_path()).await?;
    deca::layout::migrate(&pool, &config.deca_dir()).await.context("no se pudieron reorganizar los PDF")?;
    let http = reqwest::Client::builder()
        .user_agent(maps::USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let (map_queue, map_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = state::AppState {
        pool,
        config: Arc::new(config.clone()),
        http,
        login_limiter: Arc::default(),
        map_queue,
        public_url: Arc::default(),
        app_update: Arc::default(),
    };

    jobs::spawn(state.clone());
    maps::regions::spawn_worker(state.clone(), map_rx);
    maps::regions::spawn_auto_update(state.clone());
    public_url::spawn_periodic_check(state.clone());

    let app = routes::router(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("DeCA Server escuchando en http://{addr} (datos en {})", config.data_dir.display());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("apagando el servidor");
}

#[cfg(test)]
mod security_tests;

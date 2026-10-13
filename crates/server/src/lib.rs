mod admin;
mod api_access;
mod audit;
mod auth;
pub mod config;
mod crypto;
mod database;
mod error;
mod library;
mod login_admission;
mod media_delivery;
mod metrics;
mod rooted_fs;
mod routes;
mod runtime_lock;
mod secrets;
mod storage;
mod upload_commit;
mod web_assets;

#[cfg(test)]
mod database_tests;

pub mod offline;

use anyhow::Result;
use config::Config;
use routes::AppState;
use std::sync::Arc;
use storage::LocalStorage;
pub(crate) async fn build_state(
    config: &Config,
    pool: sqlx::SqlitePool,
    storage: LocalStorage,
    credentials_key: &[u8; 32],
) -> Result<AppState> {
    let administrator = Arc::new(xcss::admin_core::AdministratorService::new(
        xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    ));
    let administrator_origin = if config.development {
        xcss::admin_auth::AdministratorOriginMode::LoopbackDevelopmentHttp
    } else {
        xcss::admin_auth::AdministratorOriginMode::ProductionHttps
    };
    let web_directory = match &config.development_web_dir {
        Some(path) => {
            anyhow::ensure!(
                config.development && env!("XSZS_SOURCE_REVISION") == "unbound",
                "XSZS_DEVELOPMENT_WEB_DIR requires an unbound development build"
            );
            Some(Arc::new(xcss::web_assets::DirectoryAssets::new(path)?))
        }
        None => None,
    };
    Ok(AppState {
        web_directory,
        secrets: crypto::SecretBox::new(credentials_key),
        pool,
        storage,
        config: config.clone(),
        login_admission: login_admission::LoginAdmission::default(),
        upload_admission: routes::UploadAdmission::new(
            config.upload_global_concurrency,
            config.upload_per_instance_concurrency,
        ),
        completion_admission: routes::UploadAdmission::new(2, 1),
        administrator,
        administrator_origin,
    })
}

use tracing::info;
pub async fn serve(config: Config) -> Result<()> {
    initialize_logging(&config.log_level)?;
    let signals = xcss::server_runtime::ProcessSignals::install()?;
    let listeners = xcss::server_runtime::BoundListeners::bind([config.bind])?;
    let transport = xcss::server_runtime::HttpServer::new(listeners, signals);
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    offline::validate_runtime_inputs(&config).await?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    let _common_lock = directory.try_instance_lock()?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    let _runtime_lock =
        runtime_lock::RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    let credentials_key = offline::validate_state(&config).await?;
    enable_runtime_logging(&config.root)?;
    tracing::info!(event = "common.config.loaded");
    let pool = database::connect_validated_location(&_runtime_lock.database_url()?).await?;
    _runtime_lock.verify_original_paths()?;
    let storage = LocalStorage::open_existing(config.storage_dir.clone()).await?;
    let state = build_state(&config, pool, storage, &credentials_key).await?;
    let reconciliation = upload_commit::reconcile_all(&state).await?;
    info!(
        recovered = reconciliation.recovered,
        marked_unknown = reconciliation.marked_unknown,
        orphan_stages_removed = reconciliation.orphan_stages_removed,
        orphan_blobs_removed = reconciliation.orphan_blobs_removed,
        errors = reconciliation.errors,
        "upload commit reconciliation finished"
    );
    let health_pool = state.pool.clone();
    let health_storage = state.storage.clone();
    let reconcile_state = state.clone();
    let runtime =
        xcss::server_runtime::ServerRuntime::builder(xcss::server_runtime::ProductDescriptor {
            id: "xszs".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            common_revision: env!("XCSS_REVISION").into(),
            profile: "server-control-plane".into(),
            capabilities: vec![
                "embedded-web".into(),
                "admin-persistent".into(),
                "server-runtime".into(),
                "server-health".into(),
                "chunked-upload".into(),
                "media-library".into(),
            ],
        })
        .with_schema_identity(database::current_schema_identity()?)
        .register_health_check(
            "database",
            xcss::server_runtime::health_check(move || {
                let pool = health_pool.clone();
                async move { database::ready(&pool).await }
            }),
        )
        .register_health_check(
            "storage",
            xcss::server_runtime::health_check(move || {
                let storage = health_storage.clone();
                async move { storage.probe_readiness().await.is_ok() }
            }),
        )
        .register_background_task(
            "upload-reconciliation",
            xcss::server_runtime::TaskCriticality::Degrading,
            move |mut shutdown| async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    tokio::select! {
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { return Ok(()); }
                        }
                        _ = interval.tick() => {
                            upload_commit::reconcile_all(&reconcile_state)
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                    }
                }
            },
        )
        .register_background_task(
            "shutdown-log",
            xcss::server_runtime::TaskCriticality::Degrading,
            |mut shutdown| async move {
                if !*shutdown.borrow() {
                    let _ = shutdown.changed().await;
                }
                tracing::info!(event = "common.runtime.shutdown_started");
                Ok(())
            },
        )
        .build()
        .await?;
    let runtime_handle = runtime.handle();
    tracing::info!(event = "common.runtime.started");
    let app =
        routes::router(state, runtime_handle.clone())?.layer(axum::middleware::from_fn_with_state(
            "xszs".to_owned(),
            xcss::server_cli::service_identity_middleware,
        ));
    info!(address = %config.bind, "xszs server listening");
    runtime.serve(transport, app).await?;
    tracing::info!(event = "common.runtime.stopped");
    Ok(())
}

static LOG_LAYER: std::sync::OnceLock<xcss::log::XcssStructuredLayer> = std::sync::OnceLock::new();
fn initialize_logging(log_level: &str) -> anyhow::Result<()> {
    use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
    let layer = xcss::log::XcssStructuredLayer::new("xszs")?;
    LOG_LAYER
        .set(layer.clone())
        .map_err(|_| anyhow::anyhow!("logging already initialized"))?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_new(log_level)?)
        .with(layer)
        .try_init()?;
    Ok(())
}
fn enable_runtime_logging(root: &std::path::Path) -> anyhow::Result<()> {
    xcss::state_file::PrivateStateDirectory::open(root.join("logs"))?;
    let file = xcss::log::RotatingLogFile::open(
        root.join("logs"),
        "xszs",
        xcss::log::LogRetention::default(),
    )?;
    LOG_LAYER
        .get()
        .ok_or_else(|| anyhow::anyhow!("logging is unavailable"))?
        .set_rotating_file(file)?;
    Ok(())
}

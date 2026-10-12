mod admin;
mod api_access;
mod audit;
mod auth;
mod config;
mod crypto;
mod database;
mod doctor;
mod error;
mod library;
mod login_admission;
mod media_delivery;
mod metrics;
mod release;
mod rooted_fs;
mod routes;
mod runtime_lock;
mod secrets;
mod storage;
mod upload_commit;
mod web_assets;

#[cfg(test)]
mod database_tests;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use routes::AppState;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use storage::LocalStorage;
use tracing::info;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let json = std::env::args_os().any(|argument| argument == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return if error.print().is_ok() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            };
        }
        Err(_) => {
            let error = xcss::server_cli::ErrorEnvelope::with_code(
                xcss::server_cli::ErrorCode::new("invalid_cli_input").unwrap(),
                "Command arguments do not satisfy the current CLI contract; use --help.",
            );
            return xcss::server_cli::report_error(&error, json, 2);
        }
    };
    match execute(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let envelope = if let Some(error) = error.downcast_ref::<xcss::config::ConfigError>() {
                error.envelope()
            } else if let Some(error) = error.downcast_ref::<xcss::server_cli::CliError>() {
                error.0.clone()
            } else if let Some(error) = error.downcast_ref::<xcss::state_file::Error>() {
                xcss::server_cli::state_error(error)
            } else if let Some(error) = error.downcast_ref::<xcss::server_cli::SnapshotError>() {
                xcss::server_cli::snapshot_error(error)
            } else {
                if !json {
                    eprintln!("{error:#}");
                }
                xcss::server_cli::ErrorEnvelope::with_code(
                    xcss::server_cli::ErrorCode::new("current_state_invalid").unwrap(),
                    "The command could not validate or operate on the current configuration and data.",
                )
            };
            xcss::server_cli::report_error(&envelope, json, 1)
        }
    }
}

async fn query_status(bind: std::net::SocketAddr, json: bool) -> anyhow::Result<()> {
    let report = xcss::server_cli::query_status(bind, "xszs")
        .await
        .map_err(xcss::server_cli::CliError)?;
    if !report.ready {
        return Err(
            xcss::server_cli::CliError(xcss::server_cli::ErrorEnvelope::with_code(
                xcss::server_cli::ErrorCode::new("service_not_ready").unwrap(),
                "The service answered but its business readiness checks failed.",
            ))
            .into(),
        );
    }
    xcss::server_cli::print_report(&report, json)?;
    Ok(())
}

async fn execute(mut cli: Cli) -> Result<()> {
    let command = cli.command;
    match &command {
        Command::WebAssets => {
            xcss::web_assets::verify_embedded(
                web_assets::ASSETS,
                web_assets::MANIFEST,
                web_assets::DIGEST,
            )?;
            print!("{}", web_assets::MANIFEST);
            return Ok(());
        }
        Command::ReleaseIdentity => {
            println!("{}", release::identity_json()?);
            return Ok(());
        }
        Command::ReleaseVerify { root } => {
            let identity = release::verify(root)?;
            println!("{}", release::verification_line(&identity));
            return Ok(());
        }
        Command::ReleaseVerifyInstalled { root } => {
            let identity = release::verify_installed(root)?;
            println!("{}", release::verification_line(&identity));
            return Ok(());
        }
        Command::Run => {}
        Command::Doctor
        | Command::Reconcile {
            command: ReconcileCommand::Scan,
        }
        | Command::Init { .. }
        | Command::Check
        | Command::Admin { .. }
        | Command::Config {
            command: ConfigCommand::Validate,
        }
        | Command::Status => {}
    }
    cli.config = config::selected_config_path(cli.config.as_deref())?;
    let config = Config::from_sources(
        cli.config.as_deref(),
        cli.data_dir.as_deref(),
        cli.bind,
        cli.development,
    )?;
    xcss::web_assets::verify_embedded(
        web_assets::ASSETS,
        web_assets::MANIFEST,
        web_assets::DIGEST,
    )?;
    initialize_logging(&config.log_level)?;
    match command {
        Command::Doctor => {
            let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
            xcss::server_cli::runtime_allowed(directory.path())
                .map_err(xcss::server_cli::CliError)?;
            let _common_lock = directory.try_maintenance_lock()?;
            xcss::server_cli::runtime_allowed(directory.path())
                .map_err(xcss::server_cli::CliError)?;
            println!("{}", serde_json::to_string(&doctor::run(&config)?)?);
            return Ok(());
        }
        Command::Init { username } => {
            let username = xcss::admin_auth::normalize_administrator_username(&username)?;
            let password = read_password_from_stdin()?;
            initialize(&config, &username, &password).await?;
            tracing::info!(event = "common.initialization.completed");
            println!(
                "{}",
                serde_json::json!({"status":"initialized", "ready":false})
            );
            return Ok(());
        }
        Command::Config {
            command: ConfigCommand::Validate,
        } => {
            println!(
                "{}",
                serde_json::json!({"status":"valid", "state_paths":[config.data_dir, config.storage_dir], "sources":config.sources})
            );
            return Ok(());
        }
        Command::Check => {
            let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
            xcss::server_cli::runtime_allowed(directory.path())
                .map_err(xcss::server_cli::CliError)?;
            let _lock = directory.try_maintenance_lock()?;
            validate_existing_configuration(&config).await?;
            println!(
                "{}",
                serde_json::json!({"status":"ok", "schema_identity":database::current_schema_identity()?})
            );
            return Ok(());
        }
        Command::Admin {
            command: AdminCommand::ResetPassword { username },
        } => {
            reset_administrator_password(&config, &username).await?;
            println!("{}", serde_json::json!({"status":"password-reset"}));
            return Ok(());
        }
        Command::Status => {
            query_status(config.bind, cli.json).await?;
            return Ok(());
        }
        Command::Reconcile {
            command: ReconcileCommand::Scan,
        } => {
            let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
            xcss::server_cli::runtime_allowed(directory.path())
                .map_err(xcss::server_cli::CliError)?;
            let _common_lock = directory.try_maintenance_lock()?;
            xcss::server_cli::runtime_allowed(directory.path())
                .map_err(xcss::server_cli::CliError)?;
            let _runtime_lock =
                runtime_lock::RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
            validate_existing_configuration(&config).await?;
            let pool = database::connect_validated_location(&_runtime_lock.database_url()?).await?;
            _runtime_lock.verify_original_paths()?;
            let storage = LocalStorage::open_existing(config.storage_dir.clone()).await?;
            let state = build_state(&config, pool, storage).await?;
            println!(
                "{}",
                serde_json::to_string(&upload_commit::reconcile_all(&state).await?)?
            );
            return Ok(());
        }
        Command::Run => {}
        Command::WebAssets
        | Command::ReleaseIdentity
        | Command::ReleaseVerify { .. }
        | Command::ReleaseVerifyInstalled { .. } => {
            unreachable!("release commands return before configuration is loaded")
        }
    }
    let signals = xcss::server_runtime::ProcessSignals::install()?;
    let listeners = xcss::server_runtime::BoundListeners::bind([config.bind])?;
    let transport = xcss::server_runtime::HttpServer::new(listeners, signals);
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    validate_existing_configuration(&config).await?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    let _common_lock = directory.try_instance_lock()?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    validate_existing_configuration(&config).await?;
    enable_runtime_logging(&config.data_dir)?;
    tracing::info!(event = "common.config.loaded");
    let _runtime_lock =
        runtime_lock::RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    validate_existing_configuration(&config).await?;
    let pool = database::connect_validated_location(&_runtime_lock.database_url()?).await?;
    _runtime_lock.verify_original_paths()?;
    let storage = LocalStorage::open_existing(config.storage_dir.clone()).await?;
    let state = build_state(&config, pool, storage).await?;
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
                async move {
                    sqlx::query_scalar::<_, i64>("SELECT 1")
                        .fetch_one(&pool)
                        .await
                        .is_ok_and(|value| value == 1)
                }
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

async fn build_state(
    config: &Config,
    pool: sqlx::SqlitePool,
    storage: LocalStorage,
) -> Result<AppState> {
    let administrator = Arc::new(xcss::admin_core::AdministratorService::new(
        xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    ));
    use xcss::admin_core::AdministratorStore as _;
    anyhow::ensure!(
        administrator.store().administrator_count().await? > 0,
        "service is not initialized; run init to create the first administrator"
    );
    administrator.store().validate_all_administrators().await?;
    let persistent_secrets = secrets::load(&config.data_dir)?;
    crypto::validate_persisted_authorizations(&pool, &persistent_secrets.credentials_key).await?;
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
        secrets: crypto::SecretBox::new(&persistent_secrets.credentials_key),
        pool,
        storage,
        config: config.clone(),
        login_admission: login_admission::LoginAdmission::default(),
        upload_admission: routes::UploadAdmission::new(
            config.upload_global_concurrency,
            config.upload_per_account_concurrency,
        ),
        administrator,
        administrator_origin,
    })
}

fn current_time_micros() -> Result<u64> {
    let value = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
    u64::try_from(value).context("current time is outside SQLite range")
}

#[derive(Parser)]
#[command(
    name = "xszs",
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " source=", env!("XSZS_SOURCE_REVISION"), " xcss=", env!("XCSS_REVISION")),
    about = "xszs server with local administration"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    bind: Option<std::net::SocketAddr>,
    #[arg(long, global = true, action = clap::ArgAction::Set)]
    development: Option<bool>,
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long, default_value = "admin")]
        username: String,
    },
    Run,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Status,
    Check,
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
    Doctor,
    Reconcile {
        #[command(subcommand)]
        command: ReconcileCommand,
    },
    WebAssets,
    ReleaseIdentity,
    ReleaseVerify {
        root: PathBuf,
    },
    ReleaseVerifyInstalled {
        root: PathBuf,
    },
}
#[derive(Subcommand)]
enum AdminCommand {
    ResetPassword {
        #[arg(long)]
        username: String,
    },
}
#[derive(Subcommand)]
enum ConfigCommand {
    Validate,
}
#[derive(Subcommand)]
enum ReconcileCommand {
    Scan,
}

async fn initialize(config: &Config, username: &str, password: &str) -> Result<()> {
    xcss::admin_auth::validate_password(password)?;
    xcss::server_cli::create_empty_private_directory(&config.data_dir)
        .map_err(xcss::server_cli::CliError)?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    let _maintenance = directory.try_maintenance_lock()?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    xcss::server_cli::create_runtime_log_directory(&config.data_dir)
        .map_err(xcss::server_cli::CliError)?;
    LocalStorage::new(config.storage_dir.clone()).await?;
    let _runtime_lock =
        runtime_lock::RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    secrets::initialize(&config.data_dir)?;
    database::initialize(&config.database_url)?;
    database::validate_current_database(&database::database_path(&config.database_url)?)?;
    let pool = database::connect_validated_location(&_runtime_lock.database_url()?).await?;
    _runtime_lock.verify_original_paths()?;
    let service = xcss::admin_core::AdministratorService::new(
        xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    );
    service
        .bootstrap_administrator(username, password, current_time_micros()?)
        .await?;
    xcss::sqlite::checkpoint(&pool).await?;
    pool.close().await;
    Ok(())
}

async fn reset_administrator_password(config: &Config, username: &str) -> Result<()> {
    let username = xcss::admin_auth::normalize_administrator_username(username)?;
    let password = read_password_from_stdin()?;
    xcss::admin_auth::validate_password(&password)?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(directory.path()).map_err(xcss::server_cli::CliError)?;
    let _maintenance = directory.try_maintenance_lock()?;
    let lock = runtime_lock::RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    validate_existing_configuration(config).await?;
    let pool = database::connect_validated_location(&lock.database_url()?).await?;
    let service = xcss::admin_core::AdministratorService::new(
        xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    );
    service
        .change_administrator_password(&username, &password, current_time_micros()?)
        .await?;
    pool.close().await;
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
fn enable_runtime_logging(data_dir: &std::path::Path) -> anyhow::Result<()> {
    xcss::server_cli::validate_runtime_log_directory(data_dir)
        .map_err(xcss::server_cli::CliError)?;
    let file = xcss::log::RotatingLogFile::open(
        data_dir.join("logs"),
        "xszs",
        xcss::log::LogRetention::default(),
    )?;
    LOG_LAYER
        .get()
        .ok_or_else(|| anyhow::anyhow!("logging is unavailable"))?
        .set_rotating_file(file)?;
    Ok(())
}

async fn validate_existing_configuration(config: &Config) -> anyhow::Result<()> {
    xcss::server_cli::validate_runtime_log_directory(&config.data_dir)
        .map_err(xcss::server_cli::CliError)?;
    xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    database::validate_current_database(&database::database_path(&config.database_url)?)?;
    database::integrity_and_foreign_key_check(&database::database_path(&config.database_url)?)?;
    LocalStorage::open_existing(config.storage_dir.clone()).await?;
    let snapshot =
        xcss::server_cli::open_validation_snapshot(database::database_path(&config.database_url)?)
            .await?;
    let administrator = xcss::admin_sqlite::SqliteAdministratorStore::new(snapshot.pool().clone());
    use xcss::admin_core::AdministratorStore as _;
    anyhow::ensure!(
        administrator.administrator_count().await? > 0,
        "administrator initialization is required"
    );
    administrator.validate_all_administrators().await?;
    crypto::validate_persisted_authorizations(
        snapshot.pool(),
        &secrets::load(&config.data_dir)?.credentials_key,
    )
    .await?;
    snapshot.close().await;
    Ok(())
}

fn read_password_from_stdin() -> anyhow::Result<String> {
    use std::io::Read as _;

    let mut bytes = Vec::new();
    std::io::stdin()
        .take(4097)
        .read_to_end(&mut bytes)
        .context("read administrator password from standard input")?;
    anyhow::ensure!(
        bytes.len() <= 4096,
        "administrator password input is too large"
    );
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    anyhow::ensure!(
        !bytes.contains(&b'\n') && !bytes.contains(&b'\r'),
        "administrator password must be one line"
    );
    String::from_utf8(bytes).context("administrator password must be UTF-8")
}

#[cfg(test)]
mod command_tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn core_commands_and_maintenance_boundary_are_explicit() {
        for args in [
            vec!["init"],
            vec!["run"],
            vec!["config", "validate", "--json"],
            vec!["status", "--json"],
            vec!["reconcile", "scan"],
            vec!["--help"],
            vec!["--version"],
        ] {
            let result = Cli::try_parse_from(std::iter::once("xszs").chain(args));
            if let Err(error) = result {
                assert!(
                    matches!(
                        error.kind(),
                        clap::error::ErrorKind::DisplayHelp
                            | clap::error::ErrorKind::DisplayVersion
                    ),
                    "{error}"
                );
            }
        }
        for args in [
            vec!["backup", "create"],
            vec!["restore"],
            vec!["migrate"],
            vec!["run", "--release-root", "/tmp/release"],
        ] {
            assert!(Cli::try_parse_from(std::iter::once("xszs").chain(args)).is_err());
        }
    }
}

//! Offline product state operations linked only by the separately signed setup helper.
use crate::{
    config::Config, crypto, database, runtime_lock::RuntimeLock, secrets, storage::LocalStorage,
};
use std::path::Path;
use xcss::contracts::SchemaIdentity;
use xcss::offline::{CheckReport, ConfigReport};
fn load(root: &Path, config: &Path) -> anyhow::Result<crate::config::Config> {
    crate::config::Config::from_root_sources(root, config)
}
fn private_runtime_directories(root: &Path) -> anyhow::Result<()> {
    xcss::state_file::PrivateStateDirectory::open(root.join("logs"))?;
    xcss::state_file::PrivateStateDirectory::open(root.join("run"))?;
    Ok(())
}
fn report(config: &crate::config::Config, path: &Path) -> anyhow::Result<ConfigReport> {
    let mut state_paths = vec![
        path.to_path_buf(),
        config.data_dir.clone(),
        config.root.join("logs"),
    ];
    if !config.storage_dir.starts_with(&config.data_dir) {
        state_paths.push(config.storage_dir.clone());
    }
    state_paths.sort();
    state_paths.dedup();
    let mut writable_paths = vec![
        config.data_dir.clone(),
        config.root.join("logs"),
        config.root.join("run"),
    ];
    if !config.storage_dir.starts_with(&config.data_dir) {
        writable_paths.push(config.storage_dir.clone());
    }
    writable_paths.sort();
    writable_paths.dedup();
    Ok(ConfigReport {
        product: "xszs".into(),
        root: config.root.clone(),
        data_dir: config.data_dir.clone(),
        state_paths,
        schema_identity: database::current_schema_identity()?,
        bind: config.bind,
        writable_paths,
        shared_paths: Vec::new(),
    })
}
pub async fn validate_config(root: &Path, config: &Path) -> anyhow::Result<ConfigReport> {
    report(&load(root, config)?, config)
}
pub async fn check(root: &Path, config: &Path) -> anyhow::Result<CheckReport> {
    let value = load(root, config)?;
    validate_state(&value).await?;
    Ok(CheckReport {
        config: report(&value, config)?,
        initialized: true,
    })
}
pub(crate) fn current_time_micros() -> anyhow::Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_micros(),
    )?)
}
pub async fn migrate(
    root: &Path,
    config: &Path,
    data_dir: &Path,
    source_schema: &SchemaIdentity,
    target_schema: &SchemaIdentity,
) -> anyhow::Result<()> {
    let expected = database::current_schema_identity()?;
    anyhow::ensure!(
        source_schema == target_schema && target_schema == &expected,
        "no explicit migration exists for this schema transition"
    );
    anyhow::ensure!(
        load(root, config)?.data_dir == data_dir,
        "target data_dir differs from the verified upgrade plan"
    );
    check(root, config).await?;
    Ok(())
}
pub async fn initialize(
    root: &Path,
    config: &Path,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let username = xcss::admin_auth::normalize_administrator_username(username)?;
    xcss::admin_auth::validate_password(password)?;
    let config = load(root, config)?;
    private_runtime_directories(root)?;
    xcss::server_cli::create_empty_private_directory(&config.data_dir)
        .map_err(xcss::server_cli::CliError)?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    let _maintenance = directory.try_maintenance_lock()?;
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    xcss::server_cli::create_empty_private_directory(&config.storage_dir)
        .map_err(xcss::server_cli::CliError)?;
    LocalStorage::new(config.storage_dir.clone()).await?;
    secrets::initialize(&config.data_dir)?;
    database::initialize(&config.database_url)?;
    let lock = RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    let pool = database::connect_validated_location(&lock.database_url()?).await?;
    let result = async {
        lock.verify_original_paths()?;
        let service = xcss::admin_core::AdministratorService::new(
            xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
        );
        service
            .bootstrap_administrator(&username, password, current_time_micros()?)
            .await?;
        xcss::sqlite::checkpoint(&pool).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    pool.close().await;
    result
}
pub async fn reset_password(
    root: &Path,
    config: &Path,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let username = xcss::admin_auth::normalize_administrator_username(username)?;
    xcss::admin_auth::validate_password(password)?;
    let config = load(root, config)?;
    let directory = xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    let _maintenance = directory.try_maintenance_lock()?;
    xcss::server_cli::runtime_allowed(&config.data_dir).map_err(xcss::server_cli::CliError)?;
    validate_state(&config).await?;
    let lock = RuntimeLock::acquire(&config.database_url, &config.storage_dir)?;
    let pool = database::connect_validated_location(&lock.database_url()?).await?;
    let service = xcss::admin_core::AdministratorService::new(
        xcss::admin_sqlite::SqliteAdministratorStore::new(pool.clone()),
    );
    let result = service
        .change_administrator_password(&username, password, current_time_micros()?)
        .await;
    pool.close().await;
    result.map_err(anyhow::Error::from)
}

/// Validate existing paths and keys without copying or opening the database.
#[doc(hidden)]
pub async fn validate_runtime_inputs(config: &Config) -> anyhow::Result<()> {
    private_runtime_directories(&config.root)?;
    xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    secrets::load(&config.data_dir)?;
    database::require_secure_database_file(&database::database_path(&config.database_url)?)?;
    LocalStorage::open_existing(config.storage_dir.clone()).await?;
    Ok(())
}

#[doc(hidden)]
pub async fn validate_state(config: &Config) -> anyhow::Result<[u8; 32]> {
    crate::web_assets::verify()?;
    private_runtime_directories(&config.root)?;
    xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    let database_path = database::database_path(&config.database_url)?;
    let temporary_root = config.root.join("run");
    database::require_secure_database_file(&database_path)?;
    let key = secrets::load(&config.data_dir)?.credentials_key;
    LocalStorage::open_existing(config.storage_dir.clone()).await?;
    let snapshot = tokio::task::spawn_blocking(move || {
        xcss::sqlite::ValidationSnapshot::capture_in(database_path, temporary_root)
    })
    .await??
    .into_pool_with_connection_limits(database::connection_limits())
    .await?;
    let result = async {
        xcss::sqlite::require_pool_current_schema(
            snapshot.pool(),
            &database::current_schema_identity()?,
        )
        .await?;
        xcss::platform_db::require_current_platform_metadata(
            snapshot.pool(),
            "server-control-plane",
        )
        .await?;
        xcss::sqlite::integrity_check(snapshot.pool()).await?;
        xcss::sqlite::foreign_key_check(snapshot.pool()).await?;
        let administrator =
            xcss::admin_sqlite::SqliteAdministratorStore::new(snapshot.pool().clone());
        use xcss::admin_core::AdministratorStore as _;
        anyhow::ensure!(
            administrator.administrator_count().await? > 0,
            "administrator initialization is required"
        );
        administrator.validate_all_administrators().await?;
        crypto::validate_persisted_authorizations(snapshot.pool(), &key).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    snapshot.close().await;
    result?;
    Ok(key)
}

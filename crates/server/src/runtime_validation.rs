//! Checks of already installed runtime state; never initializes or repairs.
use crate::{config::Config, crypto, database, secrets, storage::LocalStorage};
fn private_runtime_directories(root: &std::path::Path) -> anyhow::Result<()> {
    xcss::state_file::PrivateStateDirectory::open(root.join("logs"))?;
    xcss::state_file::PrivateStateDirectory::open(root.join("run"))?;
    Ok(())
}
#[cfg(test)]
pub(crate) fn current_time_micros() -> anyhow::Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_micros(),
    )?)
}

/// Validate existing paths and keys without copying or opening the database.
pub async fn validate_runtime_inputs(config: &Config) -> anyhow::Result<()> {
    private_runtime_directories(&config.root)?;
    xcss::state_file::PrivateStateDirectory::open(&config.data_dir)?;
    secrets::load(&config.data_dir)?;
    database::require_secure_database_file(&database::database_path(&config.database_url)?)?;
    LocalStorage::open_existing(config.storage_dir.clone()).await?;
    Ok(())
}

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

use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, ensure};
use sqlx::{
    SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use xcss::schema_identity::SchemaIdentity;

#[cfg(test)]
use sqlx::Connection;
#[cfg(test)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(all(test, unix))]
use std::os::unix::fs::OpenOptionsExt;

use crate::database_definition::{APPLICATION, SCHEMA_APPLICATION_VERSION};
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const CURRENT_SCHEMA: &str = include_str!("../../../schema/generated/current_schema.sql");
pub(crate) use crate::database_definition::{
    SCHEMA_REVISION as CURRENT_SCHEMA_REVISION, SCHEMA_SHA256 as CURRENT_SCHEMA_SHA256,
};

#[cfg(test)]
pub(crate) async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    validate_current_database(&database_path(database_url)?)?;
    connect_validated_location(database_url).await
}

/// Only call after validating the original path and retaining its RuntimeLock.
/// This accepts the lock's descriptor-rooted URL without recapturing a source
/// database that SQLx may already have opened in this process.
pub(crate) async fn connect_validated_location(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(database_path(database_url)?)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT)
        .synchronous(SqliteSynchronous::Full);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .after_connect(|connection, _| {
            Box::pin(async move {
                harden_connection(connection)
                    .await
                    .map_err(|error| sqlx::Error::Io(std::io::Error::other(error)))
            })
        })
        .connect_with(options)
        .await?;
    // Revalidate after SQLx opens the production generation. A correctly
    // configured process holds RuntimeLock across both operations.
    if let Err(error) =
        xcss::sqlite::require_pool_current_schema(&pool, &current_schema_identity()?).await
    {
        pool.close().await;
        return Err(error.into());
    }
    if let Err(error) =
        xcss::platform_db::require_current_platform_metadata(&pool, "server-control-plane").await
    {
        pool.close().await;
        return Err(error.into());
    }
    Ok(pool)
}

#[cfg(test)]
pub(crate) fn initialize(database_url: &str) -> anyhow::Result<()> {
    let path = database_path(database_url)?;
    require_real_parent(&path)?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            require_absent_sidecars(&path)?;
            initialize_current_database(&path)
        }
        Err(error) => Err(error.into()),
        Ok(_) => {
            anyhow::bail!("database already exists; initialization never overwrites existing data")
        }
    }
}

pub(crate) fn connection_limits() -> xcss::sqlite::ConnectionLimits {
    xcss::sqlite::ConnectionLimits::new(2 * 1024 * 1024)
}

pub(crate) async fn ready(pool: &SqlitePool) -> bool {
    let Ok(expected) = current_schema_identity() else {
        return false;
    };
    xcss::sqlite::require_pool_current_schema(pool, &expected)
        .await
        .is_ok()
        && xcss::platform_db::require_current_platform_metadata(pool, "server-control-plane")
            .await
            .is_ok()
}

pub(crate) async fn harden_connection(connection: &mut SqliteConnection) -> anyhow::Result<()> {
    xcss::sqlite::apply_connection_limits(connection, connection_limits()).await?;
    xcss::sqlite::enable_defensive(connection).await?;
    sqlx::raw_sql("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA mmap_size=0;")
        .execute(connection)
        .await?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_current_database(path: &Path) -> anyhow::Result<()> {
    require_secure_database_file(path)?;
    validate_current_snapshot(xcss::sqlite::ValidationSnapshot::capture(path)?)
}

#[cfg(test)]
fn validate_current_snapshot(snapshot: xcss::sqlite::ValidationSnapshot) -> anyhow::Result<()> {
    xcss::sqlite::block_on_sqlite_connection(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(snapshot.database_path())
                .create_if_missing(false)
                .busy_timeout(Duration::from_secs(2)),
        )
        .await
        .context("open private SQLite current-schema snapshot")?;
        let result = async {
            harden_connection(&mut connection).await?;
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            connection
                .lock_handle()
                .await?
                .set_progress_handler(1000, move || std::time::Instant::now() < deadline);
            sqlx::query("PRAGMA query_only=ON")
                .execute(&mut connection)
                .await?;
            validate_current_connection(&mut connection).await
        }
        .await;
        let closed = connection.close().await;
        result?;
        closed?;
        Ok(())
    })
}

pub(crate) fn database_path(database_url: &str) -> anyhow::Result<PathBuf> {
    let value = database_url
        .strip_prefix("sqlite://")
        .context("DATABASE_URL must use the sqlite scheme")?;
    ensure!(!value.is_empty(), "SQLite database path must not be empty");
    ensure!(value != ":memory:", "in-memory SQLite is not supported");
    ensure!(
        !value.contains('\0'),
        "SQLite database path must not contain NUL"
    );
    let path = PathBuf::from(value);
    ensure!(path.is_absolute(), "SQLite database path must be absolute");
    ensure!(
        path.file_name().is_some(),
        "SQLite database path must name a file"
    );
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "SQLite database path must not contain parent traversal"
    );
    Ok(path)
}

#[cfg(test)]
fn initialize_current_database(path: &Path) -> anyhow::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let reserved = options
        .open(path)
        .context("create current SQLite database")?;
    reserved.sync_all()?;
    drop(reserved);
    let result = xcss::sqlite::block_on_sqlite_connection(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(path).create_if_missing(false)
                .busy_timeout(BUSY_TIMEOUT).synchronous(SqliteSynchronous::Full),
        ).await?;
        let result = async {
            harden_connection(&mut connection).await?;
            let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await?;
            sqlx::raw_sql(CURRENT_SCHEMA).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO _common_platform_metadata(singleton,platform_generation,platform_schema_revision,profile,created_at_micros) VALUES(1,1,1,'server-control-plane',?)")
                .bind(0_i64).execute(&mut *transaction).await?;
            let actual = xcss::sqlite::schema_fingerprint(&mut *transaction).await?;
            ensure!(actual == CURRENT_SCHEMA_SHA256, "compiled current schema fingerprint mismatch");
            sqlx::query("INSERT INTO product_metadata(singleton,application,application_version,schema_revision,schema_sha256) VALUES(1,?,?,?,?)")
                .bind(APPLICATION).bind(SCHEMA_APPLICATION_VERSION).bind(CURRENT_SCHEMA_REVISION).bind(CURRENT_SCHEMA_SHA256)
                .execute(&mut *transaction).await?;
            validate_current_connection(&mut transaction).await?;
            transaction.commit().await?;
            Ok::<_, anyhow::Error>(())
        }.await;
        let closed = connection.close().await;
        result?;
        closed?;
        Ok::<_, anyhow::Error>(())
    }).and_then(|()| { File::open(path)?.sync_all()?; sync_parent(path) });
    if result.is_err() {
        for candidate in sqlite_generation_paths(path) {
            let _ = fs::remove_file(candidate);
        }
        let _ = sync_parent(path);
    }
    result
}

#[cfg(test)]
fn require_absent_sidecars(path: &Path) -> anyhow::Result<()> {
    for candidate in sqlite_generation_paths(path).into_iter().skip(1) {
        match fs::symlink_metadata(&candidate) {
            Ok(_) => anyhow::bail!(
                "SQLite main file is absent but its generation contains a sidecar; refusing initialization"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
async fn validate_current_connection(connection: &mut SqliteConnection) -> anyhow::Result<()> {
    xcss::sqlite::require_current_schema(connection, &current_schema_identity()?)
        .await
        .context("database is not the exact current xszs schema")?;
    Ok(())
}

pub(crate) fn current_schema_identity() -> anyhow::Result<SchemaIdentity> {
    SchemaIdentity::new(
        APPLICATION,
        SCHEMA_APPLICATION_VERSION,
        u64::try_from(CURRENT_SCHEMA_REVISION).context("schema revision must not be negative")?,
        CURRENT_SCHEMA_SHA256,
    )
    .context("compiled xszs schema identity is invalid")
}

pub(crate) fn require_secure_database_file(path: &Path) -> anyhow::Result<()> {
    require_real_parent(path)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("SQLite database does not exist: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "SQLite database must be a regular file without symbolic links"
    );
    #[cfg(unix)]
    ensure!(
        metadata.nlink() == 1,
        "SQLite database must not have hard-link aliases"
    );
    Ok(())
}

fn require_real_parent(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("SQLite database must have a parent")?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => current.push("."),
            Component::Normal(value) => current.push(value),
            Component::ParentDir => anyhow::bail!("SQLite path must not contain parent traversal"),
        }
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("SQLite parent does not exist: {}", current.display()))?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "SQLite path must not traverse symbolic links or special files"
        );
    }
    Ok(())
}

#[cfg(test)]
fn sqlite_generation_paths(path: &Path) -> [PathBuf; 4] {
    [
        path.to_path_buf(),
        sqlite_sidecar(path, "-wal"),
        sqlite_sidecar(path, "-shm"),
        sqlite_sidecar(path, "-journal"),
    ]
}

#[cfg(test)]
fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
fn sync_parent(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row as _;
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn a_working_connection_with_altered_schema_metadata_is_not_ready() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("current.sqlite3");
        let url = format!("sqlite://{}", database.display());
        initialize_current_database(&database).unwrap();
        let pool = connect(&url).await.unwrap();
        assert!(ready(&pool).await);
        sqlx::query("UPDATE product_metadata SET schema_revision = 2")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT 1")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert!(!ready(&pool).await);
        pool.close().await;
    }

    #[tokio::test]
    async fn current_schema_fingerprint_matches_compiled_contract() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("current.sqlite3");
        initialize_current_database(&database).unwrap();
        validate_current_database(&database).unwrap();
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&database).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut connection = fixture_connection(&database).await;
        let metadata: (String, String, i64, String) = sqlx::query(
            "SELECT application, application_version, schema_revision, schema_sha256
                 FROM product_metadata WHERE singleton = 1",
        )
        .try_map(|row: sqlx::sqlite::SqliteRow| {
            Ok((
                row.try_get(0)?,
                row.try_get(1)?,
                row.try_get(2)?,
                row.try_get(3)?,
            ))
        })
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(
            metadata,
            (
                APPLICATION.to_string(),
                SCHEMA_APPLICATION_VERSION.to_string(),
                CURRENT_SCHEMA_REVISION,
                CURRENT_SCHEMA_SHA256.to_string()
            )
        );
        connection.close().await.unwrap();
    }

    async fn fixture_connection(path: &Path) -> SqliteConnection {
        SqliteConnection::connect_with(
            &sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn foreign_database_without_metadata_is_rejected_without_changing_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("foreign.sqlite3");
        let mut connection = fixture_connection(&database).await;
        sqlx::raw_sql(
            "CREATE TABLE unrelated_records(id TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO unrelated_records VALUES('record', 'foreign');",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        connection.close().await.unwrap();
        assert_rejected_without_byte_changes(&database).await;
    }

    #[tokio::test]
    async fn existing_empty_file_is_not_initialized_or_modified() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("existing-empty.sqlite3");
        File::create(&database).unwrap();
        assert_rejected_without_byte_changes(&database).await;
        assert_eq!(fs::metadata(database).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn orphan_sidecar_prevents_initialization_without_byte_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("missing-main.sqlite3");
        let wal = sqlite_sidecar(&database, "-wal");
        fs::write(&wal, b"orphan-wal-evidence").unwrap();
        let before = fs::read(&wal).unwrap();
        let error = initialize(&format!("sqlite://{}", database.display())).unwrap_err();
        assert!(format!("{error:#}").contains("sidecar"));
        assert!(!database.exists());
        assert_eq!(fs::read(wal).unwrap(), before);
    }

    #[tokio::test]
    async fn noncurrent_wal_generation_is_rejected_without_changing_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("noncurrent-wal.sqlite3");
        let mut connection = fixture_connection(&database).await;
        sqlx::raw_sql(
            "PRAGMA journal_mode=WAL;
                 PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE accounts(id TEXT PRIMARY KEY, username TEXT NOT NULL);
                 INSERT INTO accounts VALUES('unknown-user', 'noncurrent-format');",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        assert!(sqlite_sidecar(&database, "-wal").exists());
        assert_rejected_without_byte_changes(&database).await;
        connection.close().await.unwrap();
    }

    #[tokio::test]
    async fn current_schema_committed_only_in_wal_is_validated_without_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("current-wal.sqlite3");
        let mut connection = fixture_connection(&database).await;
        sqlx::raw_sql("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::raw_sql(CURRENT_SCHEMA)
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO _common_platform_metadata(\
                 singleton,platform_generation,platform_schema_revision,profile,created_at_micros\
                 ) VALUES(1,1,1,'server-control-plane',?)",
        )
        .bind(0_i64)
        .execute(&mut connection)
        .await
        .map(|result| result.rows_affected())
        .unwrap();
        let fingerprint = xcss::sqlite::schema_fingerprint(&mut connection)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO product_metadata (
                     singleton, application, application_version, schema_revision, schema_sha256
                 ) VALUES (1, ?, ?, ?, ?)",
        )
        .bind(APPLICATION)
        .bind(SCHEMA_APPLICATION_VERSION)
        .bind(CURRENT_SCHEMA_REVISION)
        .bind(fingerprint)
        .execute(&mut connection)
        .await
        .map(|result| result.rows_affected())
        .unwrap();
        assert!(sqlite_sidecar(&database, "-wal").exists());
        let before = generation_bytes(&database);
        validate_current_database(&database).unwrap();
        assert!(
            generation_bytes(&database) == before,
            "current-schema validation changed SQLite generation bytes"
        );
        connection.close().await.unwrap();
    }

    #[tokio::test]
    async fn schema_drift_committed_only_in_wal_is_rejected_without_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("drift-wal.sqlite3");
        initialize_current_database(&database).unwrap();
        let mut connection = fixture_connection(&database).await;
        sqlx::raw_sql(
            "PRAGMA journal_mode=WAL;
                 PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE unexpected_wal_table(id INTEGER);",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        assert!(sqlite_sidecar(&database, "-wal").exists());
        assert_rejected_without_byte_changes(&database).await;
        connection.close().await.unwrap();
    }

    #[tokio::test]
    async fn metadata_table_contract_is_exact_and_read_only_on_rejection() {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("metadata-contract.sqlite3");
        initialize_current_database(&database).unwrap();
        let mut connection = fixture_connection(&database).await;
        sqlx::raw_sql("PRAGMA journal_mode=DELETE;")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "DROP TABLE product_metadata;
                 CREATE TABLE product_metadata (
                     singleton INTEGER PRIMARY KEY,
                     application TEXT NOT NULL,
                     application_version TEXT NOT NULL,
                     schema_revision INTEGER NOT NULL,
                     schema_sha256 TEXT NOT NULL
                 );
                 INSERT INTO product_metadata VALUES
                     (1, '{APPLICATION}', '{}', {CURRENT_SCHEMA_REVISION}, '{CURRENT_SCHEMA_SHA256}');",
                SCHEMA_APPLICATION_VERSION
            ))).execute(&mut connection).await
            .unwrap();
        connection.close().await.unwrap();
        assert_rejected_without_byte_changes(&database).await;
    }

    #[tokio::test]
    async fn nonexact_metadata_and_schema_are_read_only_rejections() {
        for (name, statement) in [
            (
                "wrong-application",
                "UPDATE product_metadata SET application = 'another-product'",
            ),
            (
                "noncurrent-version",
                "UPDATE product_metadata SET application_version = 'noncurrent-version'",
            ),
            (
                "wrong-revision",
                "UPDATE product_metadata SET schema_revision = 2",
            ),
            (
                "wrong-fingerprint",
                "UPDATE product_metadata SET schema_sha256 = 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff'",
            ),
            (
                "schema-tamper",
                "CREATE TABLE unexpected_product_table(id INTEGER)",
            ),
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let database = temporary.path().join(format!("{name}.sqlite3"));
            initialize_current_database(&database).unwrap();
            let mut connection = fixture_connection(&database).await;
            sqlx::raw_sql("PRAGMA journal_mode=DELETE;")
                .execute(&mut connection)
                .await
                .unwrap();
            sqlx::raw_sql(sqlx::AssertSqlSafe((statement).to_owned()))
                .execute(&mut connection)
                .await
                .unwrap();
            connection.close().await.unwrap();
            assert_rejected_without_byte_changes(&database).await;
        }
    }

    async fn assert_rejected_without_byte_changes(path: &Path) {
        let before = generation_bytes(path);
        let url = format!("sqlite://{}", path.display());
        let error = connect(&url).await.unwrap_err();
        assert!(
            format!("{error:#}").contains("database")
                || format!("{error:#}").contains("product_metadata")
                || format!("{error:#}").contains("schema")
                || format!("{error:#}").contains("validation source is empty"),
            "rejection must identify the current-state boundary: {error:#}"
        );
        assert!(
            generation_bytes(path) == before,
            "current-schema rejection changed SQLite generation bytes"
        );
    }

    fn generation_bytes(path: &Path) -> BTreeMap<String, Vec<u8>> {
        sqlite_generation_paths(path)
            .into_iter()
            .filter(|candidate| candidate.exists())
            .map(|candidate| {
                (
                    candidate
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(candidate).unwrap(),
                )
            })
            .collect()
    }
}

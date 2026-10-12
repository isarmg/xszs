use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use xcss::config::{ConfigSource, EnvMapping, EnvValueKind, Override};

use anyhow::{Context, Result};

use xcss::client_ip::TrustedProxies;

const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub storage_dir: PathBuf,
    pub log_level: String,
    pub bind: SocketAddr,
    pub max_part_bytes: usize,
    pub upload_global_concurrency: usize,
    pub upload_per_account_concurrency: usize,
    pub metrics_token: Option<String>,
    pub development: bool,
    pub development_web_dir: Option<PathBuf>,
    pub trusted_proxies: TrustedProxies,
    pub sources: BTreeMap<String, ConfigSource>,
}

impl Config {
    pub fn from_sources(
        config: Option<&Path>,
        data_dir: Option<&Path>,
        bind: Option<SocketAddr>,
        development: Option<bool>,
    ) -> Result<Self> {
        let config = selected_config_path(config)?;
        let file = config
            .as_deref()
            .map(xcss::config::read_private_file)
            .transpose()?;
        let environment = xcss::config::read_environment(&ENVIRONMENT, |name| env::var(name).ok())?;
        let mut command_line = Vec::new();
        if let Some(path) = data_dir {
            command_line.push(Override::new(
                "/data_dir",
                path.to_string_lossy().into_owned(),
            ));
        }
        if let Some(bind) = bind {
            command_line.push(Override::new("/bind", bind.to_string()));
        }
        if let Some(development) = development {
            command_line.push(Override::new("/development", development));
        }
        let loaded = xcss::config::resolve_validated(
            &Settings::default(),
            file.as_deref(),
            &environment,
            &command_line,
            validate_intrinsic,
        )?;
        let settings = loaded.value;
        let data_dir = settings.data_dir.context("data_dir is required")?;
        anyhow::ensure!(data_dir.is_absolute(), "data_dir must be absolute");
        let database_url = format!("sqlite://{}", data_dir.join("xszs.sqlite3").display());
        crate::database::database_path(&database_url)?;
        let storage_dir = settings
            .storage_dir
            .unwrap_or_else(|| data_dir.join("media"));
        anyhow::ensure!(storage_dir.is_absolute(), "storage_dir must be absolute");
        anyhow::ensure!(
            !data_dir.starts_with(&storage_dir),
            "storage_dir cannot contain the internal data_dir"
        );
        anyhow::ensure!(
            !storage_dir
                .components()
                .chain(data_dir.components())
                .any(|part| part == std::path::Component::ParentDir),
            "state and storage paths cannot contain parent traversal"
        );
        let bind = settings.bind;
        anyhow::ensure!(
            (16 * 1024 * 1024..=64 * 1024 * 1024).contains(&settings.max_part_bytes),
            "max_part_bytes must be between 16 MiB and 64 MiB to support mobile uploads"
        );
        anyhow::ensure!(
            settings.upload_global_concurrency > 0 && settings.upload_per_account_concurrency > 0,
            "upload concurrency must be positive"
        );
        anyhow::ensure!(
            settings.upload_per_account_concurrency <= settings.upload_global_concurrency,
            "upload_per_account_concurrency cannot exceed upload_global_concurrency"
        );
        validate_security_mode(bind, settings.development)?;
        if let Some(path) = &settings.development_web_dir {
            anyhow::ensure!(
                settings.development && env!("XSZS_SOURCE_REVISION") == "unbound",
                "development_web_dir requires an unbound development build"
            );
            anyhow::ensure!(path.is_absolute(), "development_web_dir must be absolute");
            xcss::web_assets::DirectoryAssets::new(path)?;
        }
        let trusted_proxies = TrustedProxies::parse(&settings.trusted_proxies)?;
        Ok(Self {
            database_url,
            data_dir,
            storage_dir,
            log_level: settings.log_level,
            bind,
            max_part_bytes: settings.max_part_bytes,
            upload_global_concurrency: settings.upload_global_concurrency,
            upload_per_account_concurrency: settings.upload_per_account_concurrency,
            metrics_token: settings.metrics_token,
            development: settings.development,
            development_web_dir: settings.development_web_dir,
            trusted_proxies,
            sources: loaded.sources,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    data_dir: Option<PathBuf>,
    storage_dir: Option<PathBuf>,
    log_level: String,
    bind: SocketAddr,
    max_part_bytes: usize,
    upload_global_concurrency: usize,
    upload_per_account_concurrency: usize,
    metrics_token: Option<String>,
    development: bool,
    development_web_dir: Option<PathBuf>,
    trusted_proxies: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data_dir: Some(PathBuf::from("/var/lib/isarmg/xszs")),
            storage_dir: None,
            log_level: "info".into(),
            bind: DEFAULT_BIND.parse().expect("valid default bind"),
            max_part_bytes: 64 * 1024 * 1024,
            upload_global_concurrency: 16,
            upload_per_account_concurrency: 4,
            metrics_token: None,
            development: false,
            development_web_dir: None,
            trusted_proxies: Vec::new(),
        }
    }
}

const ENVIRONMENT: [EnvMapping<'static>; 11] = [
    EnvMapping {
        variable: "XSZS_STORAGE_DIR",
        path: "/storage_dir",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_LOG_LEVEL",
        path: "/log_level",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_DEVELOPMENT_WEB_DIR",
        path: "/development_web_dir",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_DATA_DIR",
        path: "/data_dir",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_BIND",
        path: "/bind",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_MAX_PART_BYTES",
        path: "/max_part_bytes",
        kind: EnvValueKind::UnsignedInteger,
    },
    EnvMapping {
        variable: "XSZS_UPLOAD_GLOBAL_CONCURRENCY",
        path: "/upload_global_concurrency",
        kind: EnvValueKind::UnsignedInteger,
    },
    EnvMapping {
        variable: "XSZS_UPLOAD_PER_ACCOUNT_CONCURRENCY",
        path: "/upload_per_account_concurrency",
        kind: EnvValueKind::UnsignedInteger,
    },
    EnvMapping {
        variable: "XSZS_METRICS_TOKEN",
        path: "/metrics_token",
        kind: EnvValueKind::String,
    },
    EnvMapping {
        variable: "XSZS_DEVELOPMENT",
        path: "/development",
        kind: EnvValueKind::Boolean,
    },
    EnvMapping {
        variable: "XSZS_TRUSTED_PROXIES",
        path: "/trusted_proxies",
        kind: EnvValueKind::Json,
    },
];

#[cfg(test)]
fn bind_address(value: Option<String>) -> Result<SocketAddr> {
    value
        .unwrap_or_else(|| DEFAULT_BIND.to_owned())
        .parse()
        .context("BIND must be a socket address")
}

#[cfg(test)]
fn configured_administrator_username(value: String) -> Result<String> {
    xcss::admin_auth::normalize_administrator_username(&value)
        .map_err(|error| anyhow::anyhow!("ADMIN_USERNAME is invalid: {error}"))
}

fn validate_security_mode(bind: SocketAddr, development: bool) -> Result<()> {
    if development && !bind.ip().is_loopback() {
        anyhow::bail!("DEVELOPMENT=true requires a loopback BIND address");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bind_address, configured_administrator_username, validate_security_mode};

    #[test]
    fn omitted_bind_is_exact_loopback_default() {
        assert_eq!(
            bind_address(None).unwrap(),
            "127.0.0.1:8080".parse().unwrap()
        );
        assert_eq!(
            bind_address(Some("192.0.2.10:8443".to_owned())).unwrap(),
            "192.0.2.10:8443".parse().unwrap()
        );
    }

    #[test]
    fn administrator_username_is_current_normalized_identity() {
        assert_eq!(
            configured_administrator_username(" Admin.Ops ".to_owned()).unwrap(),
            "admin.ops"
        );
        for invalid in ["ad", "admin@example.test", "-admin", "admin-", "管理员"] {
            assert!(configured_administrator_username(invalid.to_owned()).is_err());
        }
    }

    #[test]
    fn insecure_cookies_are_limited_to_explicit_loopback_development() {
        assert!(validate_security_mode("127.0.0.1:8080".parse().unwrap(), true).is_ok());
        assert!(validate_security_mode("[::1]:8080".parse().unwrap(), true).is_ok());
        assert!(validate_security_mode("0.0.0.0:8080".parse().unwrap(), true).is_err());
        assert!(validate_security_mode("127.0.0.1:8080".parse().unwrap(), false).is_ok());
        assert!(validate_security_mode("0.0.0.0:8080".parse().unwrap(), false).is_ok());
    }
}

fn validate_intrinsic(
    settings: &Settings,
    source: ConfigSource,
) -> Result<(), xcss::config::ConfigError> {
    let invalid =
        |path| xcss::config::ConfigError::new(xcss::config::Reason::InvalidValue, path, source);

    if settings
        .data_dir
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(invalid("/data_dir"));
    }
    if !(16 * 1024 * 1024..=64 * 1024 * 1024).contains(&settings.max_part_bytes) {
        return Err(invalid("/max_part_bytes"));
    }
    if settings
        .storage_dir
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(invalid("/storage_dir"));
    }
    if !matches!(
        settings.log_level.as_str(),
        "error" | "warn" | "info" | "debug" | "trace" | "off"
    ) {
        return Err(invalid("/log_level"));
    }
    for (path, value) in [
        (
            "/upload_global_concurrency",
            settings.upload_global_concurrency,
        ),
        (
            "/upload_per_account_concurrency",
            settings.upload_per_account_concurrency,
        ),
    ] {
        if value == 0 || value > tokio::sync::Semaphore::MAX_PERMITS {
            return Err(invalid(path));
        }
    }
    TrustedProxies::parse(&settings.trusted_proxies).map_err(|_| invalid("/trusted_proxies"))?;
    Ok(())
}

#[cfg(test)]
mod precedence_contract_tests {
    use super::*;
    #[test]
    fn mobile_part_budget_is_validated_in_each_configuration_layer() {
        for value in [0, 1, 16 * 1024 * 1024 - 1, 64 * 1024 * 1024 + 1] {
            let settings = Settings {
                max_part_bytes: value,
                ..Settings::default()
            };
            assert_eq!(
                validate_intrinsic(&settings, ConfigSource::File)
                    .unwrap_err()
                    .path,
                "/max_part_bytes"
            );
        }
        for value in [16 * 1024 * 1024, 64 * 1024 * 1024] {
            let settings = Settings {
                max_part_bytes: value,
                ..Settings::default()
            };
            validate_intrinsic(&settings, ConfigSource::File).unwrap();
        }
        let file = serde_json::to_vec(&serde_json::json!({"max_part_bytes": 1})).unwrap();
        let error = xcss::config::resolve_validated(
            &Settings::default(),
            Some(&file),
            &[],
            &[Override::new(
                "/max_part_bytes",
                serde_json::json!(64 * 1024 * 1024),
            )],
            validate_intrinsic,
        )
        .err()
        .unwrap();
        assert_eq!(error.path, "/max_part_bytes");
        assert_eq!(error.source, ConfigSource::File);
    }

    #[test]
    fn a_higher_priority_override_cannot_hide_an_invalid_file_value() {
        let file = serde_json::to_vec(&serde_json::json!({"upload_global_concurrency":0})).unwrap();
        let error = xcss::config::resolve_validated(
            &Settings::default(),
            Some(&file),
            &[],
            &[Override::new(
                "/upload_global_concurrency",
                serde_json::json!(16),
            )],
            validate_intrinsic,
        )
        .err()
        .expect("invalid lower layer must fail");
        assert_eq!(error.path, "/upload_global_concurrency");
        assert_eq!(error.source, ConfigSource::File);
        assert!(
            !serde_json::to_string(&error.envelope())
                .unwrap()
                .contains("private-invalid-secret")
        );
    }
}

/// Normalize the CLI file authority before private reads and resource reporting.
pub fn normalize_config_path(path: &Path) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !path
            .components()
            .any(|part| part == std::path::Component::ParentDir),
        "config path cannot contain parent traversal"
    );
    Ok(std::path::absolute(path)?)
}

#[cfg(test)]
mod unified_configuration_tests {
    use super::*;
    #[test]
    fn explicit_missing_config_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(Config::from_sources(Some(&missing), None, None, None).is_err());
    }
    #[test]
    fn defaults_and_environment_belong_to_this_server() {
        let settings = Settings::default();
        assert!(!settings.development);
        assert_eq!(
            settings.data_dir.unwrap(),
            PathBuf::from("/var/lib/isarmg/xszs")
        );
        assert!(
            ENVIRONMENT
                .iter()
                .all(|mapping| mapping.variable.starts_with("XSZS_"))
        );
        assert!(
            !ENVIRONMENT
                .iter()
                .any(|mapping| mapping.path.contains("bootstrap"))
        );
    }
    #[test]
    fn runtime_configuration_rejects_bootstrap_passwords() {
        let file = br#"{"bootstrap_admin_password":"never-persist-this"}"#;
        assert!(
            xcss::config::resolve_validated(
                &Settings::default(),
                Some(file),
                &[],
                &[],
                validate_intrinsic
            )
            .is_err()
        );
    }
    #[test]
    fn bind_precedence_is_cli_then_prefixed_environment_then_file() {
        let file = br#"{"bind":"127.0.0.1:19001"}"#;
        let environment = xcss::config::read_environment(&ENVIRONMENT, |name| {
            (name == "XSZS_BIND").then(|| "127.0.0.1:19002".into())
        })
        .unwrap();
        let loaded = xcss::config::resolve_validated(
            &Settings::default(),
            Some(file),
            &environment,
            &[Override::new("/bind", "127.0.0.1:19003")],
            validate_intrinsic,
        )
        .unwrap();
        assert_eq!(
            loaded.value.bind,
            "127.0.0.1:19003".parse::<SocketAddr>().unwrap()
        );
        let loaded = xcss::config::resolve_validated(
            &Settings::default(),
            Some(file),
            &environment,
            &[],
            validate_intrinsic,
        )
        .unwrap();
        assert_eq!(
            loaded.value.bind,
            "127.0.0.1:19002".parse::<SocketAddr>().unwrap()
        );
    }
}

/// An omitted default file is optional; explicit files and unreadable defaults fail closed.
pub fn selected_config_path(config: Option<&Path>) -> anyhow::Result<Option<PathBuf>> {
    match config {
        Some(path) => Ok(Some(normalize_config_path(path)?)),
        None => {
            let path = PathBuf::from("/etc/isarmg/xszs/config.json");
            existing_default_config(path)
        }
    }
}

fn existing_default_config(path: PathBuf) -> anyhow::Result<Option<PathBuf>> {
    match std::fs::symlink_metadata(&path) {
        Ok(_) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(test, unix))]
mod default_configuration_tests {
    use super::*;
    #[test]
    fn dangling_default_symlink_is_not_treated_as_an_absent_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        assert!(existing_default_config(path.clone()).unwrap().is_none());
        std::os::unix::fs::symlink(directory.path().join("missing.json"), &path).unwrap();
        let selected = existing_default_config(path).unwrap().unwrap();
        assert!(xcss::config::read_private_file(&selected).is_err());
    }
}

#[cfg(test)]
mod state_configuration_tests {
    use super::*;
    #[test]
    fn obsolete_state_and_secret_overrides_are_rejected() {
        for file in [
            br#"{"database_url":"sqlite:///tmp/other.db"}"#.as_slice(),
            br#"{"credentials_key":"old-key"}"#.as_slice(),
            br#"{"trusted_proxy_cidrs":"127.0.0.1"}"#.as_slice(),
        ] {
            assert!(
                xcss::config::resolve_validated(
                    &Settings::default(),
                    Some(file),
                    &[],
                    &[],
                    validate_intrinsic
                )
                .is_err()
            );
        }
    }
    #[test]
    fn log_level_is_a_simple_validated_level() {
        let file = br#"{"log_level":"invalid-secret-level"}"#;
        assert!(
            xcss::config::resolve_validated(
                &Settings::default(),
                Some(file),
                &[],
                &[],
                validate_intrinsic
            )
            .is_err()
        );
    }
}

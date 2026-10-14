//! Current configuration data shared by runtime parsing and build-time contract generation.
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub(crate) const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) storage_dir: Option<PathBuf>,
    pub(crate) log_level: String,
    pub(crate) bind: SocketAddr,
    pub(crate) max_part_bytes: usize,
    pub(crate) upload_global_concurrency: usize,
    pub(crate) upload_per_instance_concurrency: usize,
    pub(crate) metrics_token: Option<String>,
    pub(crate) development: bool,
    pub(crate) development_web_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data_dir: Some(PathBuf::from("/opt/isarmg/xszs/state")),
            storage_dir: None,
            log_level: "info".into(),
            bind: DEFAULT_BIND.parse().expect("valid default bind"),
            max_part_bytes: MAXIMUM_PART_BYTES,
            upload_global_concurrency: 16,
            upload_per_instance_concurrency: 4,
            metrics_token: None,
            development: false,
            development_web_dir: None,
        }
    }
}

pub(crate) const MINIMUM_PART_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAXIMUM_PART_BYTES: usize = 64 * 1024 * 1024;
pub(crate) fn data_path_spec() -> serde_json::Value {
    serde_json::json!({"base":"config","value":"/data_dir","fallback":null,"suffix":"","encoding":"path"})
}
pub(crate) fn storage_path_spec() -> serde_json::Value {
    let mut fallback = data_path_spec();
    fallback["suffix"] = serde_json::json!("media");
    serde_json::json!({"base":"config","value":"/storage_dir","fallback":fallback,"suffix":"","encoding":"path"})
}
pub(crate) fn intrinsic_rules() -> serde_json::Value {
    use serde_json::json;
    let mut rules = vec![
        json!({"kind":"enum","fields":["/log_level"],"arguments":{"values":["off","error","warn","info","debug","trace"]}}),
        json!({"kind":"socket-address","fields":["/bind"],"arguments":{"loopback_when":"/development"}}),
        json!({"kind":"path","fields":["/data_dir"],"arguments":{"absolute":true}}),
        json!({"kind":"path","fields":["/development_web_dir"],"arguments":{"absolute":true,"optional":true}}),
    ];
    rules.extend([
        json!({"kind":"path","fields":["/storage_dir"],"arguments":{"absolute":true,"optional":true}}),
        json!({"kind":"integer-range","fields":["/max_part_bytes"],"arguments":{"minimum":MINIMUM_PART_BYTES,"maximum":MAXIMUM_PART_BYTES}}),
        json!({"kind":"integer-range","fields":["/upload_global_concurrency"],"arguments":{"minimum":1,"maximum":tokio::sync::Semaphore::MAX_PERMITS}}),
        json!({"kind":"integer-range","fields":["/upload_per_instance_concurrency"],"arguments":{"minimum":1,"maximum":tokio::sync::Semaphore::MAX_PERMITS}}),
        json!({"kind":"integer-order","fields":["/upload_per_instance_concurrency","/upload_global_concurrency"],"arguments":{"relation":"less-or-equal"}}),
    ]);
    json!(rules)
}

pub(crate) fn current_rules(source_bound: bool) -> serde_json::Value {
    use serde_json::json;
    let mut rules = intrinsic_rules().as_array().unwrap().clone();
    rules.extend([
        json!({"kind":"root-state-paths","fields":[],"arguments":{"paths":[data_path_spec()],"protected":xcss::server_layout::ROOT_RESERVED_DIRECTORIES}}),
        json!({"kind":"external-web-override","fields":["/development_web_dir"],"arguments":{"development_pointer":"/development","source_bound":source_bound}}),
    ]);
    rules.extend([
        json!({"kind":"root-state-paths","fields":[],"arguments":{"paths":[storage_path_spec()],"protected":xcss::server_layout::ROOT_RESERVED_DIRECTORIES}}),
        json!({"kind":"path-relation","fields":[],"arguments":{"parent":data_path_spec(),"child":storage_path_spec(),"allow_child_subtree":true,"forbidden_child_first_component_prefixes":["xszs.sqlite3","secrets.json",".state-"]}}),
    ]);
    json!(rules)
}

#[cfg(test)]
pub(crate) fn validate_intrinsic(
    settings: &Settings,
    source: xcss::config::ConfigSource,
) -> Result<(), xcss::config::ConfigError> {
    let value = serde_json::to_value(settings).map_err(|_| {
        xcss::config::ConfigError::new(xcss::config::Reason::InvalidMapping, "", source)
    })?;
    // Intrinsic rules do not refer to an installation root. Runtime additionally
    // validates all root-dependent rules with its explicit physical ROOT.
    xcss::configuration_contract::validate_rules(
        &intrinsic_rules(),
        &value,
        Path::new("/configuration-validation"),
        source,
    )
}

pub(crate) fn validate(
    settings: &Settings,
    root: &Path,
    source_bound: bool,
    source: xcss::config::ConfigSource,
) -> Result<(), xcss::config::ConfigError> {
    let value = serde_json::to_value(settings).map_err(|_| {
        xcss::config::ConfigError::new(xcss::config::Reason::InvalidMapping, "", source)
    })?;
    xcss::configuration_contract::validate_rules(&current_rules(source_bound), &value, root, source)
}

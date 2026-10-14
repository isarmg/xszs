//! Source-bound current installation materials generated at build time.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use xcss::contracts::{ReleaseIdentity, SchemaIdentity};
use xcss::secret_envelope::EnvelopeDomain;
const PRODUCT: &str = "xszs";
fn root_path(value: &str) -> Value {
    json!({"base":"root","value":value,"fallback":null,"suffix":"","encoding":"path"})
}
fn data_file(value: &str) -> Value {
    let mut path = crate::config_definition::data_path_spec();
    path["suffix"] = json!(value);
    path
}
fn asset(directory: &Path, name: &str, bytes: &[u8]) -> Value {
    std::fs::write(directory.join(name), bytes).expect("write current contract asset");
    json!({"path":format!("share/contracts/{name}"),"sha256":hex::encode(Sha256::digest(bytes))})
}
fn relocate(value: &mut Value) {
    match value {
        Value::String(text) => {
            if let Some(suffix) = text.strip_prefix(&format!("/opt/isarmg/{PRODUCT}/")) {
                *text = format!("$ROOT/{suffix}")
            }
        }
        Value::Object(values) => values.values_mut().for_each(relocate),
        Value::Array(values) => values.iter_mut().for_each(relocate),
        _ => {}
    }
}
fn expand(value: &mut Value, root: &str) {
    match value {
        Value::String(text) => {
            if text.starts_with("$ROOT/") {
                *text = text.replacen("$ROOT", root, 1)
            }
        }
        Value::Object(values) => values.values_mut().for_each(|v| expand(v, root)),
        Value::Array(values) => values.iter_mut().for_each(|v| expand(v, root)),
        _ => {}
    }
}
fn config_cases(defaults: &Value) -> Vec<Value> {
    let mut cases = vec![
        json!({"name":"current-defaults","value":defaults,"valid":true}),
        json!({"name":"omitted-fields-current-defaults","value":{},"valid":true}),
    ];
    for (name, field, value, valid) in [
        (
            "unknown-property",
            "removed_timezone",
            json!("local"),
            false,
        ),
        ("unknown-log-level", "log_level", json!("invalid"), false),
        ("invalid-bind", "bind", json!("invalid"), false),
        ("data-required", "data_dir", Value::Null, false),
        ("relative-state", "data_dir", json!("relative"), false),
        (
            "protected-state",
            "data_dir",
            json!("$ROOT/maintenance/private"),
            false,
        ),
        (
            "bound-release-web",
            "development_web_dir",
            json!("$ROOT/external-web"),
            false,
        ),
        (
            "part-exceeds-bound",
            "max_part_bytes",
            json!(crate::config_definition::MAXIMUM_PART_BYTES + 1),
            false,
        ),
        (
            "per-instance-exceeds-global",
            "upload_per_instance_concurrency",
            json!(17),
            false,
        ),
        (
            "valid-business-subtree",
            "storage_dir",
            json!("$ROOT/state/media/archive"),
            true,
        ),
        (
            "business-exposes-database",
            "storage_dir",
            json!("$ROOT/state/xszs.sqlite3.private"),
            false,
        ),
    ] {
        let mut value_case = defaults.clone();
        value_case[field] = value;
        cases.push(json!({"name":name,"value":value_case,"valid":valid}));
    }
    let mut development = defaults.clone();
    development["development"] = json!(true);
    development["bind"] = json!("0.0.0.0:8080");
    cases.push(json!({"name":"development-loopback-required","value":development,"valid":false}));

    for case in &cases {
        let mut value = defaults.clone();
        for (key, setting) in case["value"].as_object().unwrap() {
            value[key] = setting.clone();
        }
        expand(&mut value, "/installation-vector");
        let valid = serde_json::from_value::<crate::config_definition::Settings>(value)
            .ok()
            .is_some_and(|settings| {
                crate::config_definition::validate(
                    &settings,
                    Path::new("/installation-vector"),
                    true,
                    xcss::config::ConfigSource::File,
                )
                .is_ok()
            });
        assert_eq!(
            valid,
            case["valid"].as_bool().unwrap(),
            "runtime configuration contradicts installation vector {}",
            case["name"]
        );
    }
    cases
}
fn secret_valid(value: Value) -> bool {
    serde_json::from_value::<crate::secret_definition::Secrets>(value).is_ok()
}
fn secret_material() -> Value {
    let valid = json!({"credentials_key":vec![9u8;32]});
    let field = "credentials_key";
    let mut unknown = valid.clone();
    unknown["unknown"] = json!(true);
    let mut wrong_length = valid.clone();
    wrong_length[field] = json!(vec![9u8; 31]);
    let cases = vec![
        json!({"name":"current-shape","input":valid,"valid":true}),
        json!({"name":"missing-field","input":{},"valid":false}),
        json!({"name":"unknown-field","input":unknown,"valid":false}),
        json!({"name":"wrong-key-length","input":wrong_length,"valid":false}),
    ];
    for case in &cases {
        assert_eq!(
            secret_valid(case["input"].clone()),
            case["valid"].as_bool().unwrap(),
            "runtime secret contradicts installation vector"
        );
    }
    json!({"name":"instance-secrets","path":data_file("secrets.json"),"format":"json",
        "fields":[{"name":field,"bytes":32,"encoding":"byte-array"}],"constants":json!({}),"validation":[],"cases":cases})
}
fn authorization_material() -> Value {
    use crate::persistent_crypto_definition::ClientAuthorizationEnvelope;
    json!({"kind":"sgev-authorization-v1","description":"Current product envelope, object binding and authorization digest",
        "parameters":{"domain":std::str::from_utf8(ClientAuthorizationEnvelope::DOMAIN).unwrap(),"revision":ClientAuthorizationEnvelope::REVISION,
            "binding":"uuid-text","key_secret":"instance-secrets.credentials_key","plaintext_bytes":36,"digest_encoding":"raw",
            "maximum_rows":2_000_000,"maximum_ciphertext_bytes":1024,"query":"SELECT id AS identity,authorization_code_enc AS ciphertext,authorization_code_hash AS digest FROM devices",},
        "cases":serde_json::from_str::<Value>(include_str!("../../../schema/validation/authorization.json")).unwrap()})
}
pub(crate) fn emit(
    destination: &Path,
    identity: &ReleaseIdentity,
    schema: &SchemaIdentity,
    ddl: &str,
) {
    let directory = destination.join("contracts");
    std::fs::create_dir_all(&directory).expect("create current materials directory");
    let mut material =
        xcss::installation_material::common(identity, schema, Some("server-control-plane"))
            .expect("public installation authority");
    let definitions = material
        .as_object_mut()
        .unwrap()
        .remove("definitions")
        .unwrap();
    asset(
        &directory,
        "common-definitions.json",
        &serde_json::to_vec(&definitions).unwrap(),
    );
    let mut database = material
        .as_object_mut()
        .unwrap()
        .remove("database")
        .unwrap();
    database["name"] = json!(PRODUCT);
    database["path"] = data_file(&format!("{PRODUCT}.sqlite3"));
    database["ddl"] = asset(&directory, "current-schema.sql", ddl.as_bytes());
    database["validation"] = json!([authorization_material()]);

    material["databases"] = json!([database]);
    let mut config = xcss::configuration_contract::generate::<crate::config_definition::Settings>()
        .expect("current owned configuration data");
    config["schema"].as_object_mut().unwrap().remove("required");
    relocate(&mut config["defaults"]);
    config["data_dir"] = crate::config_definition::data_path_spec();
    config["bind_pointer"] = json!("/bind");
    config["rules"] = crate::config_definition::current_rules(true);
    config["cases"] = json!(config_cases(&config["defaults"]));
    material["config"] = config;
    material["secrets"] = json!([secret_material()]);
    let state = crate::config_definition::data_path_spec();
    let instance = data_file(".state-instance.lock");
    let database_lock = data_file(".xszs.sqlite3.xszs.lock");
    let storage = crate::config_definition::storage_path_spec();
    let mut storage_lock = storage.clone();
    storage_lock["encoding"] = json!("sibling-lock");
    storage_lock["suffix"] = json!(".xszs.lock");
    let mut uploads = storage.clone();
    uploads["suffix"] = json!("uploads");
    let mut blobs = storage.clone();
    blobs["suffix"] = json!("blobs");
    material["resources"] = json!({"state_paths":[state,storage,root_path("config")],"writable_paths":[state,storage,storage_lock,root_path("logs"),root_path("run")],"readonly_paths":[root_path("config/config.json")],"shared_paths":[],"business_paths":[storage],"business_directories":[uploads,blobs],"config_write_paths":[],"private_directories":[state,root_path("logs"),root_path("run")],"runtime_locks":[{"path":instance,"kind":"file"},{"path":database_lock,"kind":"file"},{"path":storage_lock,"kind":"file"}]});
    material["writers"] = json!([{"role":"server","service":format!("{PRODUCT}.service"),"configuration_paths":[root_path("config/config.json")]}]);
    material["media"] = Value::Null;
    for (path, bytes) in [
        (
            "crates/server/src/database_definition.rs",
            include_bytes!("database_definition.rs").as_slice(),
        ),
        (
            "crates/server/src/config_definition.rs",
            include_bytes!("config_definition.rs").as_slice(),
        ),
        (
            "crates/server/src/secret_definition.rs",
            include_bytes!("secret_definition.rs").as_slice(),
        ),
        (
            "crates/server/src/persistent_crypto_definition.rs",
            include_bytes!("persistent_crypto_definition.rs").as_slice(),
        ),
        (
            "crates/server/src/installation_definition.rs",
            include_bytes!("installation_definition.rs").as_slice(),
        ),
        (
            "schema/product.sql",
            include_bytes!("../../../schema/product.sql").as_slice(),
        ),
        (
            "schema/validation/authorization.json",
            include_bytes!("../../../schema/validation/authorization.json").as_slice(),
        ),
    ] {
        material["authority_sources"].as_array_mut().unwrap().push(json!({"path":path,"sha256":hex::encode(Sha256::digest(bytes)),"generator":format!("{PRODUCT}/crates/server/build.rs@{}",identity.source_revision)}));
    }
    asset(
        &directory,
        "install.json",
        &serde_json::to_vec(&material).unwrap(),
    );
}

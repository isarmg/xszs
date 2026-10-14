#[path = "src/config_definition.rs"]
mod config_definition;
#[path = "src/database_definition.rs"]
mod database_definition;
#[path = "src/installation_definition.rs"]
mod installation_definition;
#[path = "src/persistent_crypto_definition.rs"]
mod persistent_crypto_definition;
#[path = "src/secret_definition.rs"]
mod secret_definition;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

const XCSS_SOURCE: &str = "git+https://github.com/isarmg/xcss.git?rev=";

fn locked_xcss_revision(lockfile: &str) -> String {
    let revisions = lockfile
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("source = \"")?.strip_suffix('"'))
        .filter_map(|source| source.strip_prefix(XCSS_SOURCE))
        .map(|source| source.split_once('#').expect("locked xcss source"))
        .map(|(requested, locked)| {
            assert_eq!(requested, locked, "xcss revision must be immutable");
            assert!(
                locked.len() == 40 && locked.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "xcss revision must be full hexadecimal"
            );
            locked.to_owned()
        })
        .collect::<BTreeSet<_>>();
    if revisions.is_empty() {
        assert_eq!(
            env::var("PROFILE").as_deref(),
            Ok("debug"),
            "release builds require a source-bound xcss dependency"
        );
        assert!(
            env::var_os("XSSI_BUILD_METADATA_DIR").is_none(),
            "release metadata cannot describe an unbound foundation"
        );
        return "unbound".into();
    }
    assert_eq!(
        revisions.len(),
        1,
        "the xcss package must use one immutable revision"
    );
    revisions.into_iter().next().expect("one xcss revision")
}

fn main() {
    println!("cargo:rerun-if-env-changed=XSSI_BUILD_METADATA_DIR");
    println!("cargo:rerun-if-env-changed=XCSS_WEB_DIST");
    println!("cargo:rerun-if-env-changed=XSZS_SOURCE_REVISION");
    let target = env::var("TARGET").expect("Cargo provides TARGET to build scripts");
    let revision = env::var("XSZS_SOURCE_REVISION").unwrap_or_else(|_| "unbound".to_owned());
    if revision != "unbound"
        && (revision.len() != 40
            || !revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    {
        panic!("XSZS_SOURCE_REVISION must be 40 lowercase hexadecimal characters");
    }
    if target != "x86_64-unknown-linux-gnu" {
        panic!("all xszs server builds require x86_64-unknown-linux-gnu");
    }
    println!("cargo:rustc-env=XSZS_SOURCE_REVISION={revision}");
    println!("cargo:rustc-env=XSZS_BUILD_TARGET={target}");
    let xcss_revision =
        locked_xcss_revision(&fs::read_to_string("../../Cargo.lock").expect("read Cargo.lock"));
    println!("cargo:rustc-env=XCSS_REVISION={xcss_revision}");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let web_root = env::var_os("XCSS_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.parent()
                .and_then(|directory| directory.parent())
                .expect("server crate within workspace")
                .join("web/dist")
        });
    xcss::web_assets::build::generate(&web_root).expect("build current embedded Web assets");
    emit_installation_metadata();
}

fn emit_installation_metadata() {
    use sha2::{Digest, Sha256};
    use xcss::contracts::{StateContract, StateResource, StateResourceKind, StateSchemaIdentity};
    let Some(destination) = std::env::var_os("XSSI_BUILD_METADATA_DIR") else {
        return;
    };
    let destination = std::path::PathBuf::from(destination);
    assert!(
        destination.is_absolute() && destination.is_dir(),
        "explicit metadata destination must exist and be absolute"
    );
    let revision = std::env::var("XSZS_SOURCE_REVISION")
        .expect("release metadata requires a source-bound build");
    let hash = database_definition::SCHEMA_SHA256;
    let schema = xcss::contracts::SchemaIdentity::new(
        database_definition::APPLICATION,
        database_definition::SCHEMA_APPLICATION_VERSION,
        u64::try_from(database_definition::SCHEMA_REVISION)
            .expect("schema revision is non-negative"),
        hash,
    )
    .expect("schema identity");
    let contract = StateContract {
        contract_version: xcss::contracts::STATE_CONTRACT_VERSION,
        application: database_definition::APPLICATION.into(),
        application_version: std::env::var("CARGO_PKG_VERSION").unwrap(),
        source_revision: revision.clone(),
        schema: Some(StateSchemaIdentity {
            revision: u64::try_from(database_definition::SCHEMA_REVISION)
                .expect("schema revision is non-negative"),
            sha256: hash.into(),
        }),
        maintenance_locks: vec![".state-instance.lock".into()],
        resources: vec![
            StateResource {
                name: "state".into(),
                kind: StateResourceKind::DataTree,
                required: true,
            },
            StateResource {
                name: "media".into(),
                kind: StateResourceKind::DataTree,
                required: true,
            },
            StateResource {
                name: "config".into(),
                kind: StateResourceKind::Configuration,
                required: true,
            },
        ],
        external_requirements: vec![],
        companion_contracts: vec![],
    };
    contract.validate().expect("current state contract");
    let bytes = serde_json::to_vec(&contract).unwrap();
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let identity = xcss::contracts::ReleaseIdentity {
        product: database_definition::APPLICATION.into(),
        version: std::env::var("CARGO_PKG_VERSION").unwrap(),
        source_revision: revision,
        target: xcss::server_target::SERVER_TARGET_TRIPLE.into(),
        state_contract_sha256: digest,
    };
    identity.validate().expect("release identity");
    std::fs::write(
        destination.join("xszs-identity.json"),
        serde_json::to_vec(&identity).unwrap(),
    )
    .unwrap();
    std::fs::write(
        destination.join("xszs-schema.json"),
        serde_json::to_vec(&schema).unwrap(),
    )
    .unwrap();
    std::fs::write(destination.join("xszs-state-contract.json"), bytes).unwrap();
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("xcss-web-assets.rs.manifest.json");
    std::fs::copy(generated, destination.join("xszs-web-assets.json"))
        .expect("copy exact embedded Web manifest");
    installation_definition::emit(
        &destination,
        &identity,
        &schema,
        include_str!("../../schema/generated/current_schema.sql"),
    );
}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Secrets {
    pub credentials_key: [u8; 32],
}

pub(crate) fn initialize(data_dir: &std::path::Path) -> anyhow::Result<()> {
    let secrets = Secrets {
        credentials_key: xcss::server_secrets::generate_key()?,
    };
    xcss::server_secrets::initialize(data_dir, &secrets)?;
    Ok(())
}

pub(crate) fn load(data_dir: &std::path::Path) -> anyhow::Result<Secrets> {
    Ok(xcss::server_secrets::load(data_dir)?)
}

use crate::secret_definition::Secrets;

pub(crate) fn load(data_dir: &std::path::Path) -> anyhow::Result<Secrets> {
    Ok(xcss::server_secrets::load(data_dir)?)
}

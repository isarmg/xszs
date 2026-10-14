//! Current secret document definition shared with installation material generation.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Secrets {
    pub credentials_key: [u8; 32],
}

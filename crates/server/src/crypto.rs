use std::sync::Arc;

use xcss::secret::{SecretBytes, SecretKey};

use crate::error::AppError;

use crate::persistent_crypto_definition::ClientAuthorizationEnvelope;

#[derive(Clone)]
pub struct SecretBox {
    master: Arc<SecretKey<32>>,
}

impl SecretBox {
    pub fn new(master: &[u8; 32]) -> Self {
        Self {
            master: Arc::new(SecretKey::new(*master)),
        }
    }

    pub fn encrypt_client_authorization(
        &self,
        instance_id: &str,
        value: &str,
    ) -> Result<Vec<u8>, AppError> {
        xcss::secret_envelope::seal::<ClientAuthorizationEnvelope>(
            &self.master,
            instance_id.as_bytes(),
            &SecretBytes::new(value.as_bytes().to_vec()),
        )
        .map_err(|_| {
            AppError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "authorization encryption failed",
            )
        })
    }

    pub fn decrypt_client_authorization(
        &self,
        instance_id: &str,
        encoded: &[u8],
    ) -> Result<String, AppError> {
        let value = xcss::secret_envelope::open::<ClientAuthorizationEnvelope>(
            &self.master,
            instance_id.as_bytes(),
            encoded,
        )
        .map_err(|_| {
            AppError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "authorization decryption failed",
            )
        })?;
        String::from_utf8(value.expose().to_vec()).map_err(|_| {
            AppError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "authorization plaintext is invalid",
            )
        })
    }
}

/// Verify every persisted device secret against its bound identity and digest.
pub async fn validate_persisted_authorizations(
    pool: &sqlx::SqlitePool,
    key: &[u8; 32],
) -> anyhow::Result<()> {
    use futures_util::TryStreamExt as _;
    use sha2::{Digest as _, Sha256};
    use sqlx::Row as _;
    let secrets = SecretBox::new(key);
    let mut rows = sqlx::query(
        "SELECT id,authorization_code_hash,authorization_code_enc FROM devices ORDER BY id",
    )
    .fetch(pool);
    while let Some(row) = rows.try_next().await? {
        let id: uuid::Uuid = row.try_get("id")?;
        let encrypted: Vec<u8> = row.try_get("authorization_code_enc")?;
        let digest: Vec<u8> = row.try_get("authorization_code_hash")?;
        let plaintext = secrets.decrypt_client_authorization(&id.to_string(), &encrypted)?;
        anyhow::ensure!(
            plaintext.len() == 36
                && plaintext
                    .bytes()
                    .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit()),
            "persisted client authorization has an invalid format"
        );
        anyhow::ensure!(
            Sha256::digest(plaintext.as_bytes()).as_slice() == digest,
            "persisted client authorization digest does not match its encrypted value"
        );
    }
    Ok(())
}

#[cfg(test)]
mod installation_vector_tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    #[test]
    fn current_authorization_material_matches_runtime_decryption() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../schema/validation/authorization.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let input = &case["input"];
            let valid = (|| -> anyhow::Result<()> {
                let key: [u8; 32] = hex::decode(input["key_hex"].as_str().unwrap())?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("bad key"))?;
                let identity = hex::decode(input["identity_hex"].as_str().unwrap())?;
                let identity = if identity.len() == 16 {
                    uuid::Uuid::from_slice(&identity)?
                } else {
                    uuid::Uuid::parse_str(std::str::from_utf8(&identity)?)?
                };
                let ciphertext = STANDARD.decode(input["ciphertext_base64"].as_str().unwrap())?;
                let plaintext = SecretBox::new(&key)
                    .decrypt_client_authorization(&identity.to_string(), &ciphertext)?;
                anyhow::ensure!(
                    plaintext.len() == 36
                        && plaintext
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
                    "invalid code"
                );
                use sha2::Digest as _;
                anyhow::ensure!(
                    hex::decode(input["digest_hex"].as_str().unwrap())?
                        == sha2::Sha256::digest(plaintext.as_bytes()).as_slice(),
                    "digest mismatch"
                );
                Ok(())
            })()
            .is_ok();
            assert_eq!(
                valid,
                case["valid"].as_bool().unwrap(),
                "runtime differs from installation case {}",
                case["name"]
            );
        }
    }
}

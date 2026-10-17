//! Backup administration DTOs; runtime bounds and query binding are checked before decoding.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub status: String,
    pub last_seen_at: String,
    pub authorization_code: String,
    pub online: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub storage_path: String,
    pub quota_bytes: u64,
    pub used_bytes: u64,
    pub instances: Vec<Instance>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Overview {
    pub total_users: u64,
    pub online_users: u64,
    pub unlimited_users: u64,
    pub used_bytes: u64,
    pub pending_bytes: u64,
    pub quota_bytes: u64,
    pub users: Vec<User>,
    pub previous_cursor: Option<String>,
    pub next_cursor: Option<String>,
}
impl crate::common::PageCursors for Overview {
    fn previous_cursor(&self) -> Option<String> {
        self.previous_cursor.clone()
    }
    fn next_cursor(&self) -> Option<String> {
        self.next_cursor.clone()
    }
}
#[derive(Serialize)]
pub struct UpdateUser {
    pub username: String,
    pub display_name: String,
    pub storage_path: String,
    pub quota_bytes: u64,
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use xcss_web::api::decode_validated_response;
    #[test]
    fn quota_is_exact_and_user_remains_bound_to_path() {
        let value = json!({"id":"a","username":"user","display_name":"A","storage_path":"/backup","created_at":"now","last_seen_at":"now","quota_bytes":9007199254740991u64,"used_bytes":1,"pending_bytes":0,"device_count":0,"resource_count":0,"instances":[]});
        let decode = |path: &str, v| {
            decode_validated_response::<User>(v, |v| crate::contracts::validate(path, v))
        };
        assert_eq!(
            decode("/api/v1/admin/users/a", value.clone())
                .unwrap()
                .quota_bytes,
            9007199254740991
        );
        assert!(decode("/api/v1/admin/users/b", value.clone()).is_err());
        let mut fractional = value.clone();
        fractional["quota_bytes"] = json!(1.5);
        assert!(decode("/api/v1/admin/users/a", fractional).is_err());
        let mut too_large = value;
        too_large["quota_bytes"] = json!(9007199254740992u64);
        assert!(decode("/api/v1/admin/users/a", too_large).is_err());
    }
    #[test]
    fn update_body_does_not_round_unchanged_quota() {
        let input = UpdateUser {
            username: "a".into(),
            display_name: "A".into(),
            storage_path: "/backup".into(),
            quota_bytes: 9007199254740991,
        };
        assert_eq!(
            xcss_web::api::encode_json_body(&input).unwrap()["quota_bytes"].as_u64(),
            Some(9007199254740991)
        );
    }
}

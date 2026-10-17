use serde_json::Value;
fn fields(v: &Value, keys: &[&str], test: fn(&Value) -> bool) -> bool {
    v.is_object() && keys.iter().all(|k| v.get(k).is_some_and(test))
}
fn integer(v: &Value) -> bool {
    v.as_u64().is_some_and(|v| v <= 9_007_199_254_740_991)
}
fn nullable_string(v: &Value) -> bool {
    v.is_null() || v.is_string()
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        let p = crate::common::date_parts(s);
        crate::common::date_validate(&p).is_ok_and(|v| v == s)
    })
}
fn cursors(v: &Value) -> bool {
    fields(v, &["previous_cursor", "next_cursor"], nullable_string)
}
fn code(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 36
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}
pub fn instance(v: &Value) -> bool {
    fields(
        v,
        &[
            "id",
            "name",
            "platform",
            "status",
            "created_at",
            "last_seen_at",
        ],
        Value::is_string,
    ) && code(&v["authorization_code"])
        && v["online"].is_boolean()
}
pub fn user(v: &Value) -> bool {
    fields(
        v,
        &[
            "id",
            "username",
            "display_name",
            "storage_path",
            "created_at",
            "last_seen_at",
        ],
        Value::is_string,
    ) && fields(
        v,
        &[
            "quota_bytes",
            "used_bytes",
            "pending_bytes",
            "device_count",
            "resource_count",
        ],
        integer,
    ) && v["instances"]
        .as_array()
        .is_some_and(|a| a.len() <= 1 && a.iter().all(instance))
}
fn query<'a>(path: &'a str, key: &str) -> Option<&'a str> {
    path.split_once('?')?.1.split('&').find_map(|item| {
        let (name, value) = item.split_once('=')?;
        (name == key).then_some(value)
    })
}
pub fn validate(path: &str, v: &Value) -> bool {
    if v.is_null() {
        return false;
    }
    let full_path = path;
    let path = path.split('?').next().unwrap_or(path);
    if path.ends_with("/overview") {
        return cursors(v)
            && fields(
                v,
                &[
                    "total_users",
                    "online_users",
                    "unlimited_users",
                    "used_bytes",
                    "pending_bytes",
                    "quota_bytes",
                ],
                integer,
            )
            && v["users"]
                .as_array()
                .is_some_and(|a| a.len() <= 50 && a.iter().all(user));
    }
    if path.ends_with("/logs") {
        let expected_start = query(full_path, "date").or_else(|| query(full_path, "start_date"));
        let expected_end = query(full_path, "end_date").or(expected_start);
        return v["instance_id"].as_str() == query(full_path, "instance_id")
            && expected_start.is_none_or(|date| v["date"] == date)
            && expected_end.is_none_or(|date| v.get("end_date").unwrap_or(&v["date"]) == date)
            && cursors(v)
            && date(&v["date"])
            && v.get("end_date")
                .is_none_or(|e| date(e) && e.as_str() >= v["date"].as_str())
            && nullable_string(&v["instance_id"])
            && v["logs"].as_array().is_some_and(|a| {
                a.len() <= 50
                    && a.iter().all(|l| {
                        fields(l, &["action", "entity_id", "occurred_at"], Value::is_string)
                            && integer(&l["sequence"])
                    })
            });
    }
    if path.contains("/users/") {
        return user(v) && path.rsplit('/').next().is_none_or(|id| v["id"] == id);
    }
    instance(v)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn log_calendar_and_envelope() {
        assert!(!validate(
            "/api/v1/admin/logs",
            &json!({"date":"2023-02-30","end_date":"2023-02-30","instance_id":null,"logs":[],"previous_cursor":null,"next_cursor":null})
        ));
    }
    #[test]
    fn logs_bind_requested_scope_and_date_range() {
        let value = json!({"date":"2032-12-30","end_date":"2032-12-31","instance_id":"fixture-id","logs":[],"next_cursor":null,"previous_cursor":null});
        assert!(validate(
            "/api/v1/admin/logs?start_date=2032-12-30&end_date=2032-12-31&instance_id=fixture-id",
            &value
        ));
        for path in [
            "/api/v1/admin/logs?start_date=2032-12-30&end_date=2032-12-31",
            "/api/v1/admin/logs?date=2032-12-30&instance_id=fixture-id",
            "/api/v1/admin/logs?start_date=2032-12-29&end_date=2032-12-31&instance_id=fixture-id",
            "/api/v1/admin/logs?start_date=2032-12-30&end_date=2032-12-31&instance_id=other",
        ] {
            assert!(!validate(path, &value));
        }
    }
}

use serde_json::json;

use crate::net::client::Api;
use crate::support::consts::{CARD_FIELDS, USER_CARD};
use crate::support::error::{AppError, AppResult};

pub fn fields_body() -> serde_json::Value {
    json!({ "fields": CARD_FIELDS.to_vec() })
}

pub async fn purge(api: &Api, token: &str) -> AppResult<bool> {
    if token.trim().is_empty() {
        return Err(AppError::api("the memory purge needs the access token"));
    }
    let reply = api.delete(USER_CARD, token, fields_body()).await?;
    Ok(cleared(&reply))
}

pub async fn purge_quiet(api: &Api, token: &str) -> Option<bool> {
    match purge(api, token).await {
        Ok(cleared) => Some(cleared),
        Err(err) => {
            crate::support::log::lr(&format!(
                "The memory card could not be cleared, the next turn may recall the old profile: {}",
                err
            ));
            None
        }
    }
}

pub fn cleared(value: &serde_json::Value) -> bool {
    if let Some(status) = value.get("status").and_then(|v| v.as_str()) {
        if status.eq_ignore_ascii_case("success") || status.eq_ignore_ascii_case("ok") {
            return true;
        }
    }
    for key in ["deleted", "cleared", "removed"] {
        if let Some(flag) = value.get(key).and_then(|v| v.as_bool()) {
            return flag;
        }
    }
    value.get("message").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_carries_every_card_field() {
        let body = fields_body();
        let fields = body.get("fields").and_then(|v| v.as_array()).unwrap();
        assert_eq!(fields.len(), CARD_FIELDS.len());
        assert_eq!(fields[0].as_str(), Some("name"));
        assert!(fields.iter().any(|f| f.as_str() == Some("facts")));
    }

    #[test]
    fn a_success_status_counts_as_cleared() {
        assert!(cleared(&serde_json::json!({"status": "success"})));
        assert!(cleared(&serde_json::json!({"deleted": true})));
        assert!(!cleared(&serde_json::json!({"deleted": false})));
        assert!(!cleared(&serde_json::json!({})));
    }
}

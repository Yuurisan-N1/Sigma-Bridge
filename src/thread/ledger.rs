#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use serde_json::{json, Value};

use crate::net::client::Api;
use crate::support::consts::{DATA_FILE, TOKEN_REFRESH};
use crate::support::error::{AppError, AppResult};

const REFRESH_SAFETY_SECS: i64 = 120;

const IDENTITY_KEYS: [&str; 5] = ["email", "username", "name", "preferred_username", "nickname"];

#[derive(Debug, Clone)]
pub struct Session {
    pub access: String,
    pub refresh: String,
    pub user_uuid: String,
    pub expires_at: i64,
    pub label: String,
}

impl Session {
    pub fn is_fresh(&self, now: i64) -> bool {
        self.expires_at
            .saturating_sub(crate::support::consts::SESSION_EARLY_SECS)
            > now
    }
}

pub struct Ledger {
    path: PathBuf,
    tokens: Vec<String>,
    cache: Mutex<HashMap<usize, Session>>,
    fixed: bool,
}

impl Ledger {
    pub fn load(path: &Path) -> AppResult<Ledger> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(AppError::config(format!(
                    "{} could not be read: {}",
                    DATA_FILE, err
                )))
            }
        };
        let tokens = parse_tokens(&text);
        if tokens.is_empty() {
            return Err(AppError::config(format!(
                "{} holds no refresh token, add one line per account",
                DATA_FILE
            )));
        }
        Ok(Ledger {
            path: path.to_path_buf(),
            tokens,
            cache: Mutex::new(HashMap::new()),
            fixed: false,
        })
    }

    pub fn from_access(access: &str) -> AppResult<Ledger> {
        let user_uuid =
            jwt_user(access).ok_or_else(|| AppError::api("the access token carried no user id"))?;
        let expires_at = jwt_expiry(access).unwrap_or_else(|| now_unix() + 3600);
        let label = jwt_label(access).unwrap_or_else(|| crate::support::log::mask_uuid(&user_uuid));
        let mut cache = HashMap::new();
        cache.insert(
            0,
            Session {
                access: access.to_string(),
                refresh: String::new(),
                user_uuid,
                expires_at,
                label,
            },
        );
        Ok(Ledger {
            path: PathBuf::from(DATA_FILE),
            tokens: vec![access.to_string()],
            cache: Mutex::new(cache),
            fixed: true,
        })
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entry(&self, index: usize) -> Option<&str> {
        self.tokens
            .get(index % self.tokens.len())
            .map(|t| t.as_str())
    }

    pub fn cached(&self, index: usize) -> Option<Session> {
        let now = now_unix();
        let guard = self.cache.lock().ok()?;
        let found = guard.get(&index)?;
        if found.is_fresh(now) {
            Some(found.clone())
        } else {
            None
        }
    }

    pub fn store(&self, index: usize, session: Session) {
        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(index, session);
        }
    }

    pub async fn resolve(&self, index: usize, api: &Api) -> AppResult<Session> {
        if let Some(session) = self.cached(index) {
            return Ok(session);
        }

        let slot = index % self.tokens.len();
        let refresh = self.tokens[slot].clone();
        let session = self.exchange(api, &refresh).await?;
        self.write_back(slot, &session.refresh)?;
        self.store(index, session.clone());
        Ok(session)
    }

    async fn exchange(&self, api: &Api, refresh: &str) -> AppResult<Session> {
        let body = json!({ "refresh_token": refresh });
        let reply = api.post(TOKEN_REFRESH, "", body).await?;

        let access = pick(&reply, &["access_token", "accessToken"])
            .ok_or_else(|| AppError::api("the refresh reply carried no access token"))?;
        let next =
            pick(&reply, &["refresh_token", "refreshToken"]).unwrap_or_else(|| refresh.to_string());
        if access == refresh {
            return Err(AppError::api("the refresh reply reused the refresh token"));
        }

        let user_uuid = jwt_user(&access)
            .ok_or_else(|| AppError::api("the access token carried no user id"))?;
        let expires_at = jwt_expiry(&access).unwrap_or_else(|| now_unix() + 3600);
        let label = jwt_label(&access).unwrap_or_else(|| crate::support::log::mask_uuid(&user_uuid));

        Ok(Session {
            access,
            refresh: next,
            user_uuid,
            expires_at,
            label,
        })
    }

    pub fn write_back(&self, slot: usize, refresh: &str) -> AppResult<()> {
        if self.fixed {
            return Ok(());
        }
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
        if lines.len() <= slot {
            while lines.len() < slot {
                lines.push(String::new());
            }
            lines.push(refresh.to_string());
        } else {
            lines[slot] = refresh.to_string();
        }
        while lines
            .last()
            .map(|line| line.trim().is_empty())
            .unwrap_or(false)
        {
            lines.pop();
        }

        let body = format!("{}\n", lines.join("\n"));
        let tmp = self.path.with_extension("txt.tmp");
        std::fs::write(&tmp, body).map_err(|err| {
            AppError::config(format!("{} could not be written: {}", DATA_FILE, err))
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|err| {
            AppError::config(format!("{} could not be replaced: {}", DATA_FILE, err))
        })?;
        Ok(())
    }
}

pub fn parse_tokens(text: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !seen.iter().any(|t| t == trimmed) {
            seen.push(trimmed.to_string());
        }
    }
    seen
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn pick(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(|v| v.as_str()) {
            if !found.is_empty() {
                return Some(found.to_string());
            }
        }
    }
    if let Some(data) = value.get("data") {
        for key in keys {
            if let Some(found) = data.get(*key).and_then(|v| v.as_str()) {
                if !found.is_empty() {
                    return Some(found.to_string());
                }
            }
        }
    }
    None
}

pub fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(payload))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

pub fn jwt_user(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    for key in ["sub_user_id", "user_uuid", "sub", "user_id"] {
        if let Some(found) = claims.get(key).and_then(|v| v.as_str()) {
            if !found.is_empty() {
                return Some(found.to_string());
            }
        }
    }
    None
}

pub fn jwt_label(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    for key in IDENTITY_KEYS {
        if let Some(found) = claims.get(key).and_then(|v| v.as_str()) {
            let found = found.trim();
            if !found.is_empty() {
                return Some(found.to_string());
            }
        }
    }
    None
}

pub fn jwt_expiry(token: &str) -> Option<i64> {
    jwt_claims(token)?.get("exp")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_keeps_one_token_per_line() {
        let tokens = parse_tokens("a\n\nb\n# note\na\n");
        assert_eq!(tokens, vec!["a", "b"]);
    }

    #[test]
    fn a_blank_file_holds_no_token() {
        assert!(parse_tokens("  \n# x\n").is_empty());
    }

    #[test]
    fn the_entry_wraps_around() {
        let dir = std::env::temp_dir().join(format!("sig-ledger-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("data.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        let ledger = Ledger::load(&path).unwrap();
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger.entry(0), Some("one"));
        assert_eq!(ledger.entry(1), Some("two"));
        assert_eq!(ledger.entry(2), Some("one"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_a_config_error() {
        let path = std::env::temp_dir().join(format!("sig-gone-{}/data.txt", std::process::id()));
        assert!(Ledger::load(&path).is_err());
    }

    #[test]
    fn the_write_back_replaces_one_line() {
        let dir = std::env::temp_dir().join(format!("sig-write-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("data.txt");
        std::fs::write(&path, "old1\nold2\n").unwrap();
        let ledger = Ledger::load(&path).unwrap();
        ledger.write_back(1, "new2").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old1\nnew2\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_session_is_not_fresh() {
        let session = Session {
            access: "a".into(),
            refresh: "r".into(),
            user_uuid: "u".into(),
            expires_at: 1000,
            label: "u****u".into(),
        };
        assert!(!session.is_fresh(1000));
        assert!(!session.is_fresh(900));
    }

    #[test]
    fn a_live_session_is_fresh_outside_the_safety_window() {
        let session = Session {
            access: "a".into(),
            refresh: "r".into(),
            user_uuid: "u".into(),
            expires_at: 10_000,
            label: "u****u".into(),
        };
        assert!(session.is_fresh(5_000));
        assert!(!session.is_fresh(9_950));
        // menit-menit terakhir sebelum kedaluwarsa sudah masuk window refresh dini
        assert!(!session.is_fresh(9_600));
    }

    #[test]
    fn the_claims_are_read_from_a_jwt() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"sub_user_id":"u-1","exp":1795867213,"type":"access"}"#);
        let token = format!("head.{}.tail", payload);
        assert_eq!(jwt_user(&token).as_deref(), Some("u-1"));
        assert_eq!(jwt_expiry(&token), Some(1795867213));
    }

    #[test]
    fn an_opaque_token_has_no_claims() {
        assert!(jwt_claims("not-a-jwt").is_none());
        assert!(jwt_user("not-a-jwt").is_none());
    }
}

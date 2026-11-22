#![allow(dead_code)]

use serde_json::Value;

use crate::net::client::Api;
use crate::support::consts::PROFILE;
use crate::support::error::{AppError, AppResult};

pub const HERMES: &str = "hermes_limit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meter {
    pub name: String,
    pub current: u64,
    pub remaining: u64,
}

impl Meter {
    pub fn is_empty(&self) -> bool {
        self.remaining == 0
    }

    pub fn total(&self) -> u64 {
        self.current.saturating_add(self.remaining)
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub user_id: String,
    pub email: String,
    pub username: String,
    pub plan: String,
    pub hermes: Meter,
    pub meters: Vec<Meter>,
}

impl Profile {
    pub fn has_room(&self) -> bool {
        !self.hermes.is_empty()
    }

    pub fn label(&self) -> String {
        if !self.email.trim().is_empty() {
            return self.email.clone();
        }
        if !self.username.trim().is_empty() {
            return self.username.clone();
        }
        crate::support::log::mask_uuid(&self.user_id)
    }
}

pub async fn fetch(api: &Api, token: &str) -> AppResult<Profile> {
    let reply = api.get(PROFILE, token).await?;
    Profile::parse(&reply)
}

impl Profile {
    pub fn parse(value: &Value) -> AppResult<Profile> {
        let user_id = value
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if user_id.is_empty() {
            return Err(AppError::api("the profile carried no user id"));
        }

        let email = value
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let username = value
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        let plan = value
            .get("subscription")
            .and_then(|s| s.get("plan_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("free")
            .to_string();

        let mut meters: Vec<Meter> = Vec::new();
        if let Some(object) = value.as_object() {
            for (key, entry) in object {
                if !key.ends_with("_limit") {
                    continue;
                }
                let current = entry.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
                let remaining = entry.get("remaining").and_then(|v| v.as_u64()).unwrap_or(0);
                meters.push(Meter {
                    name: key.clone(),
                    current,
                    remaining,
                });
            }
        }
        meters.sort_by(|a, b| a.name.cmp(&b.name));

        let hermes = meters
            .iter()
            .find(|meter| meter.name == HERMES)
            .cloned()
            .ok_or_else(|| AppError::api("the profile carried no hermes limit"))?;

        Ok(Profile {
            user_id,
            email,
            username,
            plan,
            hermes,
            meters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{
      "user_id": "3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd",
      "subscription": {"plan_name": "free"},
      "image_gen_limit": {"current": 0, "remaining": 4},
      "hermes_limit": {"current": 3, "remaining": 93},
      "web_agent_limit": {"current": 0, "remaining": 20},
      "status": "success"
    }"#;

    #[test]
    fn the_profile_reads_the_hermes_meter() {
        let value: Value = serde_json::from_str(BODY).unwrap();
        let profile = Profile::parse(&value).unwrap();
        assert_eq!(profile.user_id, "3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd");
        assert_eq!(profile.plan, "free");
        assert_eq!(profile.hermes.current, 3);
        assert_eq!(profile.hermes.remaining, 93);
        assert!(profile.has_room());
    }

    #[test]
    fn every_limit_is_kept_and_sorted() {
        let value: Value = serde_json::from_str(BODY).unwrap();
        let profile = Profile::parse(&value).unwrap();
        let names: Vec<&str> = profile.meters.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["hermes_limit", "image_gen_limit", "web_agent_limit"]
        );
    }

    #[test]
    fn an_empty_meter_blocks_the_account() {
        let value: Value =
            serde_json::from_str(r#"{"user_id":"u","hermes_limit":{"current":96,"remaining":0}}"#)
                .unwrap();
        let profile = Profile::parse(&value).unwrap();
        assert!(profile.hermes.is_empty());
        assert!(!profile.has_room());
    }

    #[test]
    fn a_profile_without_a_user_is_refused() {
        let value: Value = serde_json::from_str(r#"{"hermes_limit":{"remaining":1}}"#).unwrap();
        assert!(Profile::parse(&value).is_err());
    }

    #[test]
    fn a_profile_without_hermes_is_refused() {
        let value: Value = serde_json::from_str(r#"{"user_id":"u"}"#).unwrap();
        assert!(Profile::parse(&value).is_err());
    }

    #[test]
    fn the_total_adds_both_halves() {
        let meter = Meter {
            name: HERMES.into(),
            current: 3,
            remaining: 93,
        };
        assert_eq!(meter.total(), 96);
    }
}

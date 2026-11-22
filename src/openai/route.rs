use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::net::client::Api;
use crate::openai::prompt::Request;
use crate::openai::tool;
use crate::proof::vault::Vault;
use crate::quota::guard::Guard;
use crate::quota::profile;
use crate::socket::client as sock;
use crate::socket::client::next_frame;
use crate::socket::summary::{self, Signal};
use crate::socket::{follow, search};
use crate::support::cfg::Setup;
use crate::support::consts::{EVENT_FOLLOWUP, EVENT_SEARCH, TURN_TIMEOUT_SECS};
use crate::support::error::{refusal, AppError, AppResult};
use crate::support::log::{lg, lr, ly};
use crate::thread::ledger::{Ledger, Session};
use crate::thread::{memory, sweep};

const MAX_ROTATION: usize = 3;
const COOLDOWN_SECS: u64 = 120;

pub struct Bridge {
    pub settings: Setup,
    pub ledger: Arc<Ledger>,
    pub guard: Arc<Guard>,
    pub vaults: Vec<Arc<Vault>>,
}

impl Bridge {
    pub fn api_for(&self, index: usize) -> AppResult<Api> {
        Api::new(self.settings.proxy_for(index))
    }

    pub fn vault_for(&self, index: usize) -> Arc<Vault> {
        self.vaults[index % self.vaults.len()].clone()
    }
}

pub struct Reply {
    pub text: String,
    pub purged: usize,
}

pub async fn answer(
    bridge: &Bridge,
    request: &Request,
    sink: Option<UnboundedSender<String>>,
) -> AppResult<Reply> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let choice = bridge.guard.acquire()?;
        let api = bridge.api_for(choice.index)?;

        let session = match bridge.ledger.resolve(choice.index, &api).await {
            Ok(session) => session,
            Err(err) => {
                bridge.guard.blip(choice.slot, COOLDOWN_SECS);
                if attempt < MAX_ROTATION {
                    lr(&format!(
                        "Account {} could not be refreshed, moving on: {}",
                        choice.slot + 1,
                        err
                    ));
                    continue;
                }
                return Err(err);
            }
        };

        let mut label = session.label.clone();
        let mut left = 0;
        if let Ok(profile) = profile::fetch(&api, &session.access).await {
            label = profile.label();
            if profile.hermes.is_empty() {
                bridge.guard.mark_exhausted(choice.slot, &profile.hermes);
                if attempt < MAX_ROTATION {
                    ly(&format!(
                        "Account {} ({}) has spent its plan limit, rotating",
                        choice.slot + 1,
                        label
                    ));
                    continue;
                }
                return Err(AppError::quota(
                    "every account has used up its plan limit, stopping",
                ));
            }
            left = profile.hermes.remaining;
            bridge.guard.mark_live(choice.slot);
        }

        match turn(bridge, request, &choice, &session, &api, sink.clone()).await {
            Ok((reply, purged, cleared)) => {
                lg(&format!(
                    "Account {} ({}) answered, {} turn(s) left, {} thread(s) swept, memory {}",
                    choice.slot + 1,
                    label,
                    left,
                    purged,
                    if cleared { "cleared" } else { "kept" }
                ));
                if let Ok(profile) = profile::fetch(&api, &session.access).await {
                    bridge.guard.mark_exhausted(choice.slot, &profile.hermes);
                }
                return Ok(Reply { text: reply, purged });
            }
            Err(err) => {
                bridge.guard.blip(choice.slot, COOLDOWN_SECS);
                if attempt < MAX_ROTATION {
                    lr(&format!(
                        "Account {} ({}) refused the turn, rotating: {}",
                        choice.slot + 1,
                        label,
                        err
                    ));
                    continue;
                }
                return Err(err);
            }
        }
    }
}

async fn turn(
    bridge: &Bridge,
    request: &Request,
    choice: &crate::quota::guard::Choice,
    session: &Session,
    api: &Api,
    sink: Option<UnboundedSender<String>>,
) -> AppResult<(String, usize, bool)> {
    let vault = bridge.vault_for(choice.index);
    let pow_session = vault
        .session(api, &session.access, &session.user_uuid)
        .await?;

    let query = compose(request);

    let mut socket = sock::open(
        &session.access,
        &session.user_uuid,
        bridge.settings.proxy_for(choice.index),
    )
    .await?;

    let payload = if query.follow_up {
        follow::payload(&follow::Turn {
            query: &query.text,
            session_id: &session.user_uuid,
            token: &session.access,
            pow_session: &pow_session,
            hash: &query.hash,
            parent_id: &query.parent_id,
        })
    } else {
        search::payload(&search::Turn {
            query: &query.text,
            session_id: &session.user_uuid,
            token: &session.access,
            pow_session: &pow_session,
        })
    };

    let event = if query.follow_up {
        EVENT_FOLLOWUP
    } else {
        EVENT_SEARCH
    };
    sock::emit(&mut socket, event, &payload).await?;

    let mut text = String::new();
    loop {
        let frame = next_frame(&mut socket, TURN_TIMEOUT_SECS).await?;
        match summary::classify(&frame) {
            Signal::Text(piece) => {
                text.push_str(&piece);
                if let Some(tx) = &sink {
                    let _ = tx.send(piece);
                }
            }
            Signal::Finished(_) => break,
            Signal::Refused(message) => {
                return Err(AppError::api(refusal(&message)));
            }
            Signal::Counter(_) | Signal::Tracked { .. } | Signal::Ignored => {}
        }
    }

    vault.spend();

    let purged = sweep::purge_quiet(api, &session.access, &session.user_uuid)
        .await
        .unwrap_or(0);
    let cleared = memory::purge_quiet(api, &session.access).await.unwrap_or(false);
    let _ = socket.close(None).await;

    Ok((text, purged, cleared))
}

struct Query {
    text: String,
    follow_up: bool,
    hash: String,
    parent_id: String,
}

pub fn tools_allowed(request: &Request) -> bool {
    match &request.tool_choice {
        Some(Value::String(mode)) => mode != "none",
        Some(Value::Object(_)) => true,
        _ => true,
    }
}

fn compose(request: &Request) -> Query {
    let mut text = String::new();
    if !request.tools.is_empty() && tools_allowed(request) {
        let block = tool::advertised(&request.tools);
        if !block.is_empty() {
            text.push_str(&block);
            text.push_str("\n\n");
        }
    }
    text.push_str(&crate::openai::prompt::flatten(request));

    Query {
        text,
        follow_up: false,
        hash: String::new(),
        parent_id: String::new(),
    }
}

pub fn visible_of(reply: &Reply, request: &Request) -> (String, Vec<Value>, &'static str) {
    if request.tools.is_empty() || !tools_allowed(request) {
        return (reply.text.trim().to_string(), Vec::new(), "stop");
    }
    let pieces = tool::parse_blocks(&reply.text);
    if tool::has_calls(&pieces) {
        let nonce = crate::support::log::nonce(crate::support::consts::TOOL_NONCE_LEN);
        let calls = tool::calls(&pieces, &nonce);
        (tool::visible_text(&pieces), calls, "tool_calls")
    } else {
        (reply.text.trim().to_string(), Vec::new(), "stop")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::prompt;

    fn request(body: &str) -> Request {
        prompt::parse(&serde_json::from_str(body).unwrap()).unwrap()
    }

    #[test]
    fn the_query_flattens_the_transcript() {
        let req = request(r#"{"messages":[{"role":"user","content":"hi"}]}"#);
        let query = compose(&req);
        assert_eq!(query.text, "user: hi");
        assert!(!query.follow_up);
    }

    #[test]
    fn the_query_leads_with_the_tool_block() {
        let req = request(
            r#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"x","description":"d"}}]}"#,
        );
        let query = compose(&req);
        assert!(query.text.starts_with("[TOOLS]"));
        assert!(query.text.contains("user: hi"));
    }

    #[test]
    fn a_tool_reply_is_split_into_text_and_calls() {
        let req = request(
            r#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"x"}}]}"#,
        );
        let reply = Reply {
            text: "Sure.\n<tool>{\"name\":\"x\",\"arguments\":{\"a\":1}}</tool>".to_string(),
            purged: 0,
        };
        let (text, calls, finish) = visible_of(&reply, &req);
        assert_eq!(text, "Sure.");
        assert_eq!(finish, "tool_calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["function"]["name"], "x");
    }

    #[test]
    fn a_plain_reply_stays_stop() {
        let req = request(
            r#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"x"}}]}"#,
        );
        let reply = Reply {
            text: "just words".to_string(),
            purged: 0,
        };
        let (text, calls, finish) = visible_of(&reply, &req);
        assert_eq!(text, "just words");
        assert!(calls.is_empty());
        assert_eq!(finish, "stop");
    }

    #[test]
    fn a_tool_choice_of_none_is_ignored() {
        let req = request(
            r#"{"messages":[{"role":"user","content":"hi"}],"tool_choice":"none","tools":[{"type":"function","function":{"name":"x"}}]}"#,
        );
        assert!(!tools_allowed(&req));
        let query = compose(&req);
        assert!(!query.text.contains("[TOOLS]"));
        let reply = Reply {
            text: "<tool>{\"name\":\"x\"}</tool>".to_string(),
            purged: 0,
        };
        let (_, calls, finish) = visible_of(&reply, &req);
        assert!(calls.is_empty());
        assert_eq!(finish, "stop");
    }

    #[test]
    fn an_object_tool_choice_still_allows_tools() {
        let req = request(
            r#"{"messages":[{"role":"user","content":"hi"}],"tool_choice":{"type":"function","function":{"name":"x"}},"tools":[{"type":"function","function":{"name":"x"}}]}"#,
        );
        assert!(tools_allowed(&req));
    }

    #[test]
    fn a_reply_without_tools_is_never_parsed() {
        let req = request(r#"{"messages":[{"role":"user","content":"hi"}]}"#);
        let reply = Reply {
            text: "<tool>{\"name\":\"x\"}</tool>".to_string(),
            purged: 0,
        };
        let (text, calls, finish) = visible_of(&reply, &req);
        assert!(text.contains("<tool>"));
        assert!(calls.is_empty());
        assert_eq!(finish, "stop");
    }
}

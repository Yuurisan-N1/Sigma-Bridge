#![allow(dead_code)]

use std::path::Path;

use crate::net::client::Api;
use crate::support::consts::DATA_FILE;
use crate::support::error::{refusal, AppResult};
use crate::support::log::{gap, lg, lr, ly, mask_uuid};
use crate::{proof, quota, thread};

pub const ACCESS_ENV: &str = "SIGMA_ACCESS";

pub fn chat_text() -> String {
    let mut args = std::env::args().skip(1);
    let mut found: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg == "--text" {
            found = args.next();
            break;
        }
        if let Some(rest) = arg.strip_prefix("--text=") {
            found = Some(rest.to_string());
            break;
        }
    }
    found
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Reply with exactly: PING".to_string())
}

pub fn access_from_env() -> Option<String> {
    std::env::var(ACCESS_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub async fn run(root: &Path) -> AppResult<()> {
    if let Some(access) = access_from_env() {
        return token_probe(&access).await;
    }

    let api = Api::new(None)?;
    let ledger = thread::ledger::Ledger::load(&root.join(DATA_FILE))?;

    ly(&format!("Probing {} refresh token(s)", ledger.len()));
    for index in 0..ledger.len() {
        let session = ledger.resolve(index, &api).await?;
        let label = match quota::profile::fetch(&api, &session.access).await {
            Ok(profile) => format!("{} ({})", profile.label(), profile.plan),
            Err(_) => session.label.clone(),
        };
        lg(&format!(
            "Account {}: {} valid for {} seconds",
            index + 1,
            label,
            session.expires_at - thread::ledger::now_unix()
        ));
    }
    gap();
    Ok(())
}

pub async fn one(root: &Path, index: usize) -> AppResult<()> {
    let api = Api::new(None)?;
    let ledger = thread::ledger::Ledger::load(&root.join(DATA_FILE))?;
    let session = ledger.resolve(index, &api).await?;

    let granted = grant(&api, &session.access, &session.user_uuid).await?;
    lg(&format!("The session token grants {} turns", granted));

    let profile = quota::profile::fetch(&api, &session.access).await?;
    lg(&format!(
        "The plan is {} and hermes stands at {}/{}",
        profile.plan,
        profile.hermes.current,
        profile.hermes.total()
    ));
    Ok(())
}

pub async fn token_probe(access: &str) -> AppResult<()> {
    let api = Api::new(None)?;
    let user_uuid = thread::ledger::jwt_user(access).ok_or_else(|| {
        crate::support::error::AppError::api("the access token carried no user id")
    })?;

    ly(&format!(
        "Probing with a raw access token for {}",
        mask_uuid(&user_uuid)
    ));

    let granted = grant(&api, access, &user_uuid).await?;
    lg(&format!(
        "The proof was redeemed, the session grants {} turns",
        granted
    ));

    let profile = quota::profile::fetch(&api, access).await?;
    lg(&format!(
        "The account reads {} on the {} plan",
        profile.label(),
        profile.plan
    ));
    lg(&format!(
        "The plan is {} and hermes stands at {}/{}",
        profile.plan,
        profile.hermes.current,
        profile.hermes.total()
    ));

    let swept = thread::sweep::purge_quiet(&api, access, &user_uuid)
        .await
        .unwrap_or(0);
    lg(&format!("The thread shelf was swept, {} removed", swept));

    let cleared = thread::memory::purge_quiet(&api, access).await.unwrap_or(false);
    lg(&format!(
        "The memory card was {}",
        if cleared { "cleared" } else { "kept" }
    ));

    gap();
    Ok(())
}

pub async fn token_chat(access: &str, text: &str) -> AppResult<()> {
    let api = Api::new(None)?;
    let user_uuid = thread::ledger::jwt_user(access).ok_or_else(|| {
        crate::support::error::AppError::api("the access token carried no user id")
    })?;

    let pow_session = granted_token(access, &user_uuid).await?;
    ly("One live turn with a fresh proof session");

    let mut socket = crate::socket::client::open(access, &user_uuid, None).await?;
    let payload = crate::socket::search::payload(&crate::socket::search::Turn {
        query: text,
        session_id: &user_uuid,
        token: access,
        pow_session: &pow_session,
    });
    crate::socket::client::emit(&mut socket, crate::support::consts::EVENT_SEARCH, &payload)
        .await?;

    let mut answer = String::new();
    loop {
        let frame = crate::socket::client::next_frame(
            &mut socket,
            crate::support::consts::TURN_TIMEOUT_SECS,
        )
        .await?;
        match crate::socket::summary::classify(&frame) {
            crate::socket::summary::Signal::Text(piece) => {
                answer.push_str(&piece);
                if answer.len() % 40 < piece.len() {
                    ly(&format!("{} characters so far", answer.len()));
                }
            }
            crate::socket::summary::Signal::Finished(_) => break,
            crate::socket::summary::Signal::Refused(message) => {
                return Err(crate::support::error::AppError::api(refusal(&message)))
            }
            _ => {}
        }
    }
    let _ = socket.close(None).await;

    lg(&format!("The answer was {} characters", answer.len()));
    let preview: String = answer.chars().take(160).collect();
    lg(&format!("It began: {}", preview));

    let swept = thread::sweep::purge_quiet(&api, access, &user_uuid)
        .await
        .unwrap_or(0);
    lr(&format!("The thread shelf was swept, {} removed", swept));

    let cleared = thread::memory::purge_quiet(&api, access).await.unwrap_or(false);
    lr(&format!(
        "The memory card was {}",
        if cleared { "cleared" } else { "kept" }
    ));
    gap();
    Ok(())
}

async fn granted_token(access: &str, user_uuid: &str) -> AppResult<String> {
    let api = Api::new(None)?;
    let challenge = proof::challenge::request(&api, access, user_uuid, "chat").await?;
    let solved = proof::solve::solve(&challenge, user_uuid)?;
    let granted = proof::redeem::redeem(
        &api,
        access,
        &challenge.challenge_id,
        &solved.proof,
        solved.ms,
    )
    .await?;
    Ok(granted.session_token)
}

async fn grant(api: &Api, access: &str, user_uuid: &str) -> AppResult<u32> {
    let challenge = proof::challenge::request(api, access, user_uuid, "chat").await?;
    let solved = proof::solve::solve(&challenge, user_uuid)?;
    lg(&format!("The proof was solved in {} ms", solved.ms));
    let granted = proof::redeem::redeem(
        api,
        access,
        &challenge.challenge_id,
        &solved.proof,
        solved.ms,
    )
    .await?;
    Ok(granted.quota)
}

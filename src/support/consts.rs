#![allow(dead_code)]

pub const CONFIG_FILE: &str = "config.json";
pub const DATA_FILE: &str = "data.txt";
pub const PROXY_FILE: &str = "proxy.txt";

pub const MODEL_ID: &str = "sigma-flash";
pub const MODEL_OWNED: &str = "sigma";

pub const API_BASE: &str = "https://bagoodex.io/api/v1";
pub const SOCKET_ORIGIN: &str = "https://bagoodex.io";
pub const SOCKET_HOST: &str = "bagoodex.io";
pub const SOCKET_PORT: u16 = 443;
pub const SOCKET_PATH: &str = "/api/socket.io";
pub const APP_ORIGIN: &str = "https://app.sigmabrowser.com";

pub const MODELS: &str = "/v1/models";
pub const CHAT: &str = "/v1/chat/completions";

pub const POW_CHALLENGE: &str = "/security/pow/challenge";
pub const POW_REDEEM: &str = "/security/pow/redeem";
pub const PROFILE: &str = "/account/profile";
pub const TOKEN_REFRESH: &str = "/account/token/refresh";
pub const THREAD_PURGE: &str = "/account/thread/delete";
pub const USER_CARD: &str = "/user-card";

pub const CARD_FIELDS: [&str; 10] = [
    "name",
    "preferred_language",
    "profession",
    "interests",
    "current_projects",
    "long_term_goals",
    "response_style",
    "response_format",
    "tone",
    "facts",
];

pub const POW_ACTION: &str = "chat";
pub const POW_SESSION_SECS: u64 = 3600;

pub const EVENT_SEARCH: &str = "search";
pub const EVENT_FOLLOWUP: &str = "followup";
pub const EVENT_SUMMARY: &str = "summary";
pub const EVENT_FINISHED: &str = "finished";
pub const EVENT_THREAD: &str = "thread_id";
pub const EVENT_COUNTER: &str = "limit_counter";
pub const EVENT_ERROR: &str = "error";
pub const EVENT_LIMIT: &str = "limit_exceeded";

pub const PLATFORM: &str = "web";

pub const BROWSER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub const CONNECT_TIMEOUT_SECS: u64 = 20;
pub const READ_TIMEOUT_SECS: u64 = 120;
pub const TURN_TIMEOUT_SECS: u64 = 180;
pub const SOCKET_OPEN_SECS: u64 = 30;

pub const POW_ATTEMPTS: u32 = 3;
pub const POW_RETRY_MILLIS: u64 = 1500;

pub const SESSION_EARLY_SECS: i64 = 600;

pub const LEDGER_ATTEMPTS: u32 = 3;
pub const LEDGER_RETRY_MILLIS: u64 = 1000;

pub const TOOL_NONCE_LEN: usize = 16;
pub const TOOL_OPEN: &str = "<tool>";
pub const TOOL_CLOSE: &str = "</tool>";

pub const STREAM_PING: &str = ": ping";
pub const STREAM_DONE: &str = "[DONE]";

pub const KEY_HEAD: usize = 8;
pub const KEY_TAIL: usize = 10;
pub const KEY_HIDE_MIN: usize = 18;

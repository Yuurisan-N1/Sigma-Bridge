#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Method, Response};
use serde_json::Value;

use crate::net::egress::Proxy;
use crate::support::consts::{
    API_BASE, APP_ORIGIN, BROWSER_AGENT, CONNECT_TIMEOUT_SECS, READ_TIMEOUT_SECS,
};
use crate::support::error::{AppError, AppResult};

pub fn build_client(proxy: Option<&Proxy>) -> AppResult<Client> {
    let mut builder = Client::builder()
        .user_agent(BROWSER_AGENT)
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(READ_TIMEOUT_SECS))
        .danger_accept_invalid_certs(false)
        .redirect(reqwest::redirect::Policy::limited(5));

    if let Some(proxy) = proxy {
        let scheme = if proxy.scheme == "socks5" {
            reqwest::Proxy::all(proxy.url())
                .map_err(|err| AppError::network(format!("the proxy could not be set: {}", err)))?
        } else {
            reqwest::Proxy::all(proxy.url())
                .map_err(|err| AppError::network(format!("the proxy could not be set: {}", err)))?
        };
        builder = builder.proxy(scheme);
    } else {
        builder = builder.no_proxy();
    }

    builder
        .build()
        .map_err(|err| AppError::network(format!("the client could not be built: {}", err)))
}

pub struct Api {
    client: Client,
}

impl Api {
    pub fn new(proxy: Option<&Proxy>) -> AppResult<Api> {
        Ok(Api {
            client: build_client(proxy)?,
        })
    }

    pub fn http(&self) -> &Client {
        &self.client
    }

    pub async fn get(&self, path: &str, token: &str) -> AppResult<Value> {
        self.call(Method::GET, path, token, None).await
    }

    pub async fn post(&self, path: &str, token: &str, body: Value) -> AppResult<Value> {
        self.call(Method::POST, path, token, Some(body)).await
    }

    pub async fn post_empty(&self, path: &str, token: &str) -> AppResult<Value> {
        self.call(Method::POST, path, token, None).await
    }

    pub async fn delete(&self, path: &str, token: &str, body: Value) -> AppResult<Value> {
        self.call(Method::DELETE, path, token, Some(body)).await
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> AppResult<Value> {
        let reply = self.send(method, path, token, body).await?;
        let status = reply.status();
        let text = reply
            .text()
            .await
            .map_err(|err| AppError::network(format!("the reply could not be read: {}", err)))?;

        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()))
        };

        if !status.is_success() {
            return Err(AppError::api(refusal_text(&value, status.as_u16())));
        }
        Ok(value)
    }

    pub async fn send(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> AppResult<Response> {
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}{}", API_BASE, path)
        };

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            HeaderName::from_static("origin"),
            HeaderValue::from_str(APP_ORIGIN)
                .unwrap_or(HeaderValue::from_static("https://app.sigmabrowser.com")),
        );
        headers.insert(
            HeaderName::from_static("referer"),
            HeaderValue::from_static("https://app.sigmabrowser.com/"),
        );
        if !token.is_empty() {
            let value = HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|_| AppError::api("the access token could not be used"))?;
            headers.insert(AUTHORIZATION, value);
        }

        let mut request = self.client.request(method, &url).headers(headers);
        if let Some(body) = body {
            request = request.json(&body);
        }

        request
            .send()
            .await
            .map_err(|err| AppError::network(format!("the request to {} failed: {}", path, err)))
    }

    pub async fn bearer(&self, path: &str, token: &str) -> AppResult<Value> {
        self.call(Method::GET, path, token, None).await
    }
}

pub use tokio_tungstenite::tungstenite::Message;

pub fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn tls_connector() -> tokio_tungstenite::Connector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tokio_tungstenite::Connector::Rustls(Arc::new(config))
}

fn refusal_text(value: &Value, status: u16) -> String {
    if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
        return crate::support::error::refusal(message);
    }
    if let Some(detail) = value.get("detail") {
        if let Some(code) = detail.get("code").and_then(|c| c.as_str()) {
            return crate::support::error::refusal(code);
        }
    }
    crate::support::error::refusal(&format!("http {}", status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_builds_without_a_proxy() {
        assert!(build_client(None).is_ok());
    }

    #[test]
    fn a_client_builds_through_a_socks_proxy() {
        let proxy = Proxy::parse("socks5://u:p@127.0.0.1:1080").unwrap();
        assert!(build_client(Some(&proxy)).is_ok());
    }

    #[test]
    fn the_api_keeps_one_client_per_egress() {
        let api = Api::new(None).unwrap();
        assert!(api.http().get("https://example.com").build().is_ok());
    }

    #[test]
    fn the_refusal_prefers_the_message_field() {
        let value = serde_json::json!({"message": "limit_exceeded metric=web_search_and_model"});
        assert_eq!(
            refusal_text(&value, 429),
            "the plan limit for the account is used up"
        );
    }

    #[test]
    fn the_refusal_falls_back_to_the_status() {
        let value = serde_json::json!({});
        let text = refusal_text(&value, 401);
        assert!(text.contains("401"));
    }
}

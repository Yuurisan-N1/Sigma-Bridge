# Sigma Upstream Interface

This document describes the remote Sigma requests implemented inside SigmaBridge. It is separate from [API.md](../docs/API.md), which documents the HTTP API SigmaBridge exposes to clients. The source uses `bagoodex.io` for the upstream HTTP API and WebSocket connection, with the Sigma Browser application origin on HTTP headers.

This describes the current source implementation. It does not claim that the upstream service publishes or guarantees this interface as a stable contract.

## Hosts and transport

| Use | Value in source |
|---|---|
| HTTP API base | `https://bagoodex.io/api/v1` |
| WebSocket URL | `wss://bagoodex.io/api/socket.io/?EIO=4&transport=websocket` |
| WebSocket `Origin` | `https://bagoodex.io` |
| HTTP `Origin` | `https://app.sigmabrowser.com` |
| HTTP `Referer` | `https://app.sigmabrowser.com/` |

The HTTP client sends `Accept: application/json`, `Content-Type: application/json`, the HTTP `Origin` and `Referer` above, and this `User-Agent`:

```text
Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36
```

Calls that use an access token also send `Authorization: Bearer <access token>`. The refresh-token request does not use a bearer token. It sends the refresh token in its JSON body.

The HTTP base URL, WebSocket host, and paths are constants in `src/support/consts.rs`, not settings in `config.json`. The HTTP client verifies TLS certificates and follows up to five redirects. The HTTP connect timeout is 20 seconds and the request timeout is 120 seconds. WebSocket opening has a 30-second timeout. The WebSocket read timeout is applied to each frame read and is 180 seconds.

When proxies are configured, the account loop selects proxies by account index, cycling through the configured proxy list. The HTTP client uses the selected proxy. The WebSocket connection code supports SOCKS proxies; for a non-SOCKS proxy, it connects directly instead.

## Account token refresh

For an account using a refresh token, the bridge sends:

```http
POST /account/token/refresh
Content-Type: application/json
```

```json
{"refresh_token":"<refresh token>"}
```

The response parser accepts an access token named `access_token` or `accessToken`, at the top level or inside `data`. It accepts a replacement refresh token named `refresh_token` or `refreshToken`, with the same wrapper handling. If the response does not include a replacement refresh token, the bridge keeps the submitted refresh token. A replacement token is written back to the corresponding line in `data.txt`.

The access-token JWT payload is decoded to find the user ID. The code checks claims in this order: `sub_user_id`, `user_uuid`, `sub`, then `user_id`. Expiration is read from the JWT `exp` claim. If that claim is missing or unreadable, the local fallback is one hour. A session is treated as fresh only while more than ten minutes remain before its expiration. The claim-reading helper decodes the JWT payload but does not verify its signature.

If `SIGMA_ACCESS` is set, the server uses that single access token instead of loading refresh tokens from `data.txt`. The code still needs to read the user ID from that token's payload. It uses the same one-hour fallback if `exp` is unavailable.

## Proof challenge and redemption

Before sending a chat turn, the bridge obtains a proof session for the `chat` action. The proof session is cached per account and reused while its local quota is above zero and its local expiration has not passed. The local expiration is set to one hour after the token is accepted.

### Requesting a challenge

```http
POST /security/pow/challenge
Authorization: Bearer <access token>
Content-Type: application/json
```

```json
{
  "user_uuid": "<user ID>",
  "action": "chat"
}
```

The response parser requires `challenge_id` and `salt`. It also reads `alg`, `mem_kib`, `iter`, `parallelism`, `hash_len`, and `expires_in`. Defaults for missing numeric parameters are `mem_kib: 19456`, `iter: 2`, `parallelism: 1`, and `hash_len: 32`.

The solver always uses Argon2id version 1.3. Its hash input is the concatenated bytes of `challenge_id` and `user_uuid`, with the decoded challenge salt and the parameters from the challenge. A zero hash length is treated as 32. Memory, iteration, and parallelism values are clamped so they are not below their minimums before hashing. The digest is encoded as Base64 URL-safe without padding. Although `alg` and `expires_in` are read, the source does not use them to select the algorithm or to set the local proof-session expiration.

### Redeeming a proof

```http
POST /security/pow/redeem
Authorization: Bearer <access token>
Content-Type: application/json
```

```json
{
  "challenge_id": "<challenge ID>",
  "proof": "<Base64 URL-safe digest>",
  "solve_ms": 0
}
```

`solve_ms` is the solver's measured duration in milliseconds. The response parser looks for a session token named `session_token`, `sessionToken`, or `token`, at the top level or inside `data`. The local proof quota is read from `quota` or `remaining`, also with `data` wrapper support. If no quota is found, the default is 5. The bridge decrements its local quota copy by one after a turn reaches the `finished` event.

## Chat turns over WebSocket

The bridge opens the WebSocket URL above using Engine.IO v4. It expects the first text frame to begin with `0`, sends `40` to connect the Socket.IO namespace, then sends this authentication frame:

```text
42{"token":"<access token>","sessionId":"<user ID>"}
```

The code does not wait for a separate authentication acknowledgment before sending a chat event.

The bridge then sends a Socket.IO event. The current server route sends each request as the `search` event. The event frame is `42` followed by a JSON array whose payload is serialized as a JSON string. The second array item is a string, not a nested JSON object. Simplified form:

```text
42["search","{\"query\":\"...\",...}"]
```

The `query` value is assembled from non-empty client messages into role-labeled text such as `system: ...` and `user: ...`, separated by newlines. If the request includes tool definitions and tool processing is enabled, a `[TOOLS]` text block is prepended. SigmaBridge does not send the original OpenAI message array as the Sigma payload.

The current `search` event payload has these fields:

| Field | Value produced by the source |
|---|---|
| `query` | Assembled role-labeled text, with a leading `[TOOLS]` block when applicable |
| `sessionId` | The account user ID |
| `platform_type` | `web` |
| `utmData` | An object with `utmSource`, `utmMedium`, `utmCampaign`, `utmTerm`, and `utmContent`, all set to `null` |
| `followupId` | `null` |
| `domain_search` | Empty object `{}` |
| `token` | Access token |
| `powSessionToken` | Redeemed proof-session token |

The source does not include a `query_type` field in this payload.

The source also contains a `followup` payload builder. It creates these fields: `query`, `sessionId`, `platform_type: "web"`, `hash`, `parent_id`, `token`, `domain_search: {}`, and `powSessionToken`. However, the current server route sets `follow_up` to `false`, so it sends `search`, not `followup`. The presence of the helper does not mean the current server route sends follow-up events.

### Events read from the socket

The turn reader combines text from `summary` events and stops when it receives `finished`.

| Event | Handling in source |
|---|---|
| `summary` | Reads text from a string payload or a `value`, `text`, or `message` field, then appends non-empty text to the answer. A string payload containing JSON is parsed when possible. |
| `finished` | Ends the turn. Its identifiers are parsed, but the server route does not use them to continue the conversation. |
| `thread_id` | Can read the hash and follow-up ID, but the server route ignores the parsed signal. |
| `limit_counter` | Can read `counter` or `remaining`, but the server route ignores the parsed signal. |
| `error`, `limit_exceeded`, `pow_required`, `unauthorized` | Treated as a refusal and returned as an error. |

Unknown events are also treated as refusals if they have a string `message` field and no `value` field. Other unknown frames or events are ignored. WebSocket ping frames are answered with pong.

## Account profile and quota

The main OpenAI route fetches the profile before each account attempt to check its quota. It fetches the profile again after a completed turn to update the account's exhausted state. The CLI also fetches it in its account and token probes.

```http
GET /account/profile
Authorization: Bearer <access token>
```

The profile parser reads top-level `user_id`, `subscription.plan_name`, and top-level fields whose names end with `_limit`. For each limit, it reads `current` and `remaining`, defaulting missing numeric values to zero. The parser also reads top-level `email` and `username` strings, trimming surrounding whitespace. Its account label prefers `email`, then `username`, then a masked user ID. When no profile label is available, the session label is taken from JWT claims in this order: `email`, `username`, `name`, `preferred_username`, then `nickname`; if none is present, it uses a masked user ID. The parser requires `user_id` and `hermes_limit`. If the plan name is missing, the local value is `free`. This parser does not unwrap a `data` wrapper.

If the pre-turn profile request succeeds and `hermes_limit.remaining` is zero, the route marks that account exhausted and may rotate to another account. If the pre-turn profile request fails, the route ignores the error and proceeds with the account. After a completed turn, profile-fetch errors are also ignored, so an error from that call alone does not mark the account exhausted. The route makes at most three account attempts; refresh or turn errors put the account on a 120-second cooldown.

## Thread cleanup

After receiving `finished`, the bridge sends a request with an empty body:

```http
POST /account/thread/delete/<user ID>/all
Authorization: Bearer <access token>
```

This request is made after the turn completes successfully. Cleanup failure is logged and does not change a completed answer into an error. The deleted count is read from `deleted`, `Deleted`, `count`, or `threads`, including when the field is inside `data`, or from a message containing a number after the word `deleted`. If no count is found, the value is treated as zero.

## User-card memory cleanup

After thread cleanup, the main OpenAI route sends a user-card cleanup request. The direct-token CLI probe and chat paths also send it after their thread cleanup request. The request is:

```http
DELETE /user-card
Authorization: Bearer <access token>
Content-Type: application/json
```

```json
{
  "fields": [
    "name",
    "preferred_language",
    "profession",
    "interests",
    "current_projects",
    "long_term_goals",
    "response_style",
    "response_format",
    "tone",
    "facts"
  ]
}
```

The source considers the card cleared if the top-level `status` is `success` or `ok` (case-insensitive), if the first present top-level boolean among `deleted`, `cleared`, and `removed` is `true`, or if a top-level `message` field exists. It does not unwrap a `data` wrapper. Request failures are logged and do not turn a completed answer into an error; the route reports a failed or unrecognized cleanup as `kept`.

## Implementation limits

The upstream paths and payload details above are limited to what this source creates and reads. They do not imply support for other Sigma endpoints, payload fields, authentication schemes, or response formats. The client-facing routes provided by the bridge remain those listed in [API.md](../docs/API.md).

Implementation references: `src/support/consts.rs`, `src/net/client.rs`, `src/thread/ledger.rs`, `src/thread/memory.rs`, `src/thread/sweep.rs`, `src/proof/challenge.rs`, `src/proof/solve.rs`, `src/proof/redeem.rs`, `src/proof/vault.rs`, `src/socket/client.rs`, `src/socket/search.rs`, `src/socket/follow.rs`, `src/socket/summary.rs`, `src/quota/profile.rs`, `src/quota/guard.rs`, `src/openai/route.rs`, and `src/cli/probe.rs`.

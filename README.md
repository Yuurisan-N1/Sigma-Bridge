<div align="center">

<img width="100%" alt="header" src="https://capsule-render.vercel.app/api?type=waving&height=210&text=Sigma%20Bridge%20Bot&fontAlign=50&fontAlignY=36&fontSize=56&desc=OpenAI-Compatible%20Local%20Bridge%20for%20Sigma%20Browser%20AI&descAlign=50&descAlignY=58"/>

<img alt="typing" src="https://readme-typing-svg.demolab.com?font=Inter&size=18&duration=3000&pause=650&center=true&vCenter=true&width=900&lines=OpenAI-Compatible+HTTP+Bridge;sigma-flash+%7C+Streaming+and+Non-Streaming;Tool+Call+Support+%7C+Function+Definitions;Multi-Account+%7C+Round-Robin+and+Sticky+Routing;Proxy+Support+%7C+Probe+Mode"/>

<p>
  <img alt="rust" src="https://img.shields.io/badge/Rust-2021-f74c00?logo=rust&logoColor=white"/>
  <img alt="api" src="https://img.shields.io/badge/API-OpenAI%20Compatible-111111"/>
  <img alt="model" src="https://img.shields.io/badge/Model-sigma--flash-111111"/>
  <img alt="build" src="https://github.com/Yuurisan-N1/Sigma-Bridge/actions/workflows/build.yml/badge.svg"/>
  <img alt="author" src="https://img.shields.io/badge/by-Yuurisandesu-111111"/>
</p>

<p>
  <b>Sigma Bridge</b> is a local OpenAI-compatible HTTP bridge for Sigma Browser AI.<br/>
  It reads refresh tokens from <code>data.txt</code>, maintains a session ledger across all accounts, and exposes a local HTTP server on port 1122 that accepts <code>/v1/chat/completions</code> and <code>/v1/models</code> requests in the OpenAI format, with streaming and non-streaming support, tool call handling, round-robin or sticky account routing, and automatic session and proof-of-work renewal per request.<br/>
  Built and distributed by <b>Yuurisandesu</b>.
</p>

</div>

---

## Table of Contents

- [Requirements](#requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Running the Bridge](#running-the-bridge)
- [Run Modes](#run-modes)
- [Connecting a Client](#connecting-a-client)
- [Download Prebuilt Binary](#download-prebuilt-binary)
- [Features](#features)
- [File Structure](#file-structure)
- [Continuous Integration](#continuous-integration)
- [Disclaimer](#disclaimer)

---

## Requirements

- Rust stable toolchain (install from [rustup.rs](https://rustup.rs), only needed if building from source)
- Git

---

## Installation

### Install Rust

**Linux / macOS:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Verify:

```bash
rustc --version
```

**Windows:**

Download and run the installer from https://rustup.rs, then reopen the terminal and verify:

```cmd
rustc --version
```

**Termux (Android):**

```bash
pkg update && pkg install proot-distro
proot-distro install ubuntu
proot-distro login ubuntu
```

Then inside Ubuntu:

```bash
apt update && apt install -y curl git build-essential pkg-config libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

---

### Clone and Build

**Clone the repository:**

```bash
git clone https://github.com/Yuurisan-N1/Sigma-Bridge.git
cd Sigma-Bridge
```

**Build the release binary:**

```bash
./run.sh build
```

---

## Configuration

### 1. Refresh Tokens (data.txt)

Fill `data.txt` with one refresh token per line. Blank lines and lines starting with `#` are ignored:

```
eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

To get a refresh token, open Sigma Browser AI in the browser, open DevTools, go to the Application tab, and copy the refresh token stored in the site's cookies or local storage.

Alternatively, set the environment variable `SIGMA_ACCESS` to a single access token and the bridge will use it directly without reading `data.txt`.

### 2. Proxy (proxy.txt)

Fill `proxy.txt` with proxies, one per line (optional, leave empty to connect directly):

```
http://user:pass@host:port
socks5://user:pass@host:port
```

Proxies are paired to accounts by index in round-robin order. Proxy credentials are masked in the log.

### 3. Bot Settings (config.json)

```json
{
  "settings": {
    "sleep_seconds": 3600,
    "round_robin": true,
    "sticky": 3,
    "port": 1122
  }
}
```

| Key | Default | Description |
|---|---|---|
| `sleep_seconds` | `3600` | Seconds between thread sweep cycles. Clamped between 60 and 86400. |
| `round_robin` | `true` | Rotate accounts on every request. Set to `false` for sticky mode. |
| `sticky` | `3` | Number of consecutive requests served by the same account before rotating. Only applies when `round_robin` is `false`. |
| `port` | `1122` | Port the bridge listens on. |

---

## Running the Bridge

Make `run.sh` executable first (Linux / macOS / Termux):

```bash
chmod +x run.sh
```

Then choose a mode:

```bash
./run.sh serve      # build if needed and start the bridge in foreground
./run.sh bg         # build if needed and run in background, logs to sigmabridge.log
./run.sh screen     # build if needed and run in a detached screen session
./run.sh stop       # stop the background or screen session
./run.sh probe      # check connectivity and account state without serving
./run.sh selftest   # offline checks, no network
./run.sh build      # build only, do not run
```

For background mode, tail the log anytime:

```bash
tail -f sigmabridge.log
```

For screen mode, attach anytime:

```bash
screen -r sigmabridge
```

Press `Ctrl+C` once to stop the bridge cleanly when running in foreground.

### Manual run

```bash
# Linux / macOS / Termux
./target/release/sigmabridge-bot

# Windows
.\target\release\sigmabridge-bot.exe
```

---

## Run Modes

| Mode | Command | Description |
|---|---|---|
| serve | `./run.sh serve` | Start the bridge and serve requests |
| bg | `./run.sh bg` | Same as serve but in background with log file |
| screen | `./run.sh screen` | Same as serve in a detached screen session |
| probe | `./run.sh probe` | Check site connectivity and account state without serving |
| selftest | `./run.sh selftest` | Offline checks, no network |
| stop | `./run.sh stop` | Stop background or screen session |
| build | `./run.sh build` | Build only |

---

## Connecting a Client

Point any OpenAI-compatible client at:

```
Base URL : http://127.0.0.1:1122/v1
API Key  : not required
Model    : sigma-flash
```

The server listens on `0.0.0.0` so it is reachable from other machines on the same network. Do not expose it to an untrusted network without a reverse proxy or firewall rule.

**curl example:**

```bash
curl http://127.0.0.1:1122/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "sigma-flash",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**curl streaming example:**

```bash
curl http://127.0.0.1:1122/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "sigma-flash",
    "stream": true,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**curl tool call example:**

```bash
curl http://127.0.0.1:1122/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "sigma-flash",
    "messages": [{"role": "user", "content": "What is the weather in Tokyo?"}],
    "tools": [{
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get current weather for a location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {"type": "string"}
          }
        }
      }
    }]
  }'
```

For the full API reference see [docs/API.md](docs/API.md).

---

## Download Prebuilt Binary

Prebuilt binaries are automatically compiled on every push to main via GitHub Actions across 3 platforms.

Download the latest binaries from the Actions page:
https://github.com/Yuurisan-N1/Sigma-Bridge/actions/workflows/build.yml

Open the latest successful run and scroll to the Artifacts section.

| Artifact | Platform |
|---|---|
| `sigmabridge-linux-x86_64` | Linux x86_64 |
| `sigmabridge-windows-x86_64` | Windows x86_64 |
| `sigmabridge-macos-aarch64` | macOS Apple Silicon |

**Linux / macOS after downloading:**

```bash
chmod +x sigmabridge-linux-x86_64
./sigmabridge-linux-x86_64
```

**Windows:**

```
.\sigmabridge-windows-x86_64.exe
```

Place the binary in the same folder as `data.txt`, `proxy.txt`, and `config.json` before running.

---

## Features

### OpenAI-Compatible API
The bridge exposes a local HTTP server that speaks the OpenAI API format on two endpoints: `GET /v1/models` and `POST /v1/chat/completions`. Any client that accepts a custom base URL works with it without modification.

### Streaming and Non-Streaming
Both `stream: true` and `stream: false` are supported. Streaming responses send SSE chunks in the OpenAI delta format and close with a `[DONE]` marker.

### Tool Call Support
Function definitions passed in the `tools` array are forwarded with the request. When the upstream model responds with a tool call, the bridge parses it out and returns it in the OpenAI `tool_calls` format with `finish_reason: tool_calls`. Setting `tool_choice: none` disables tool handling for that request.

### Multi-Account Routing
All refresh tokens in `data.txt` are loaded into a session ledger at startup. Requests are distributed across accounts either in round-robin order or in sticky mode, where the same account serves a configurable number of consecutive requests before rotating. The active account and slot are logged on each request.

### Session and PoW Renewal
Each account maintains a live access token and a proof-of-work session. Both are renewed automatically when they expire, with no request dropped during the renewal.

### Thread Sweep
Used conversation threads are purged after each completed request to keep the account state clean and avoid accumulating history.

### Probe Mode
Probe checks site connectivity, resolves the exit country of every proxy in `proxy.txt`, and reports the profile state of each account without serving any requests.

### Proxy Support
Proxies are loaded from `proxy.txt` and assigned to accounts by position in round-robin order. Running without proxies is fully supported.

---

## File Structure

```text
Sigma-Bridge/
├── .github/
│   └── workflows/
│       └── build.yml                    # CI: 3-platform matrix build and test
├── assets/
│   ├── icon.ico                         # Windows binary icon
│   └── icon.png                         # App icon
├── docs/
│   └── API.md                           # Full API reference for the bridge endpoints
├── src/
│   ├── cli/
│   │   ├── dispatch.rs                  # CLI mode selector and usage text
│   │   ├── mod.rs
│   │   ├── probe.rs                     # Probe mode: connectivity and account state check
│   │   └── selftest.rs                  # Offline self-test checks
│   ├── net/
│   │   ├── client.rs                    # HTTP API client and request helpers
│   │   ├── egress.rs                    # Proxy parser and exit country check
│   │   └── mod.rs
│   ├── openai/
│   │   ├── error.rs                     # OpenAI-format error response builders
│   │   ├── mod.rs
│   │   ├── prompt.rs                    # Request parser and transcript flattener
│   │   ├── route.rs                     # Request router, account selector, reply assembler
│   │   ├── server.rs                    # TCP listener, HTTP parser, response writer
│   │   ├── sse.rs                       # SSE chunk builder and model list response
│   │   └── tool.rs                      # Tool definition formatter and call parser
│   ├── proof/
│   │   ├── challenge.rs                 # PoW challenge fetch
│   │   ├── mod.rs
│   │   ├── redeem.rs                    # PoW solution submit and session token receive
│   │   ├── solve.rs                     # SHA-256 nonce solver
│   │   └── vault.rs                     # PoW session cache and renewal logic
│   ├── quota/
│   │   ├── guard.rs                     # Account selector with round-robin and sticky modes
│   │   ├── mod.rs
│   │   └── profile.rs                   # Account profile and quota meter fetch
│   ├── run/
│   │   ├── mod.rs
│   │   └── serve.rs                     # Bridge boot: ledger load, vault init, server start
│   ├── socket/
│   │   ├── client.rs                    # WebSocket connection open, emit, and frame read
│   │   ├── follow.rs                    # Follow-up turn payload builder
│   │   ├── mod.rs
│   │   ├── search.rs                    # Search turn payload builder
│   │   └── summary.rs                   # Event stream classifier and text collector
│   ├── support/
│   │   ├── banner.rs                    # ASCII banner and terminal title
│   │   ├── cfg.rs                       # Config loader, proxy reader, and setup builder
│   │   ├── consts.rs                    # App and network constants
│   │   ├── error.rs                     # Error types and result alias
│   │   ├── log.rs                       # Colored log helpers, nonce generator, countdown
│   │   └── mod.rs
│   ├── thread/
│   │   ├── ledger.rs                    # Session ledger: token load, resolve, and refresh
│   │   ├── mod.rs
│   │   ├── sweep.rs                     # Thread purge after each completed request
│   │   └── memory.rs                    # User-card memory cleanup
│   └── main.rs                          # Entry point, Ctrl+C handler, mode dispatch
├── upstream/
│   └── UPSTREAM.md                      # Notes on the upstream service
├── .gitignore
├── build.rs                             # Build script: embeds icon into Windows binary
├── Cargo.lock
├── Cargo.toml                           # Project manifest and dependencies
├── config.json                          # Sleep, routing mode, sticky count, and port settings
├── data.txt                             # Refresh tokens, one per line
├── HOWTO.txt                            # Quick start guide
├── LICENSE                              # GNU AGPL v3.0
├── Makefile                             # make targets: build, release, run, start, clean, size
├── proxy.txt                            # Proxy list (optional)
├── README.md                            # This file
├── run.sh                               # Run helper: serve, bg, screen, probe, selftest, stop, build
└── SECURITY.md                          # Security policy
```

---

## Continuous Integration

The GitHub Actions workflow runs on pushes to `main` or `master` and on manual dispatch. It builds and tests the release binary on all three platforms and uploads the artifacts.

| Platform | Artifact |
|---|---|
| Linux | `sigmabridge-linux-x86_64` |
| Windows | `sigmabridge-windows-x86_64` |
| macOS | `sigmabridge-macos-aarch64` |

---

## Disclaimer

This tool is built for educational and technical exploration purposes. Use it wisely and at your own responsibility.

---

<div align="center">
<img width="100%" alt="footer" src="https://capsule-render.vercel.app/api?type=waving&height=120&section=footer"/>
</div>
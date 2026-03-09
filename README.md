```

   █████████    █████████   ██████████   ██████████ ███████████
  ███▒▒▒▒▒███  ███▒▒▒▒▒███ ▒▒███▒▒▒▒███ ▒▒███▒▒▒▒▒█▒▒███▒▒▒▒▒███
 ███     ▒▒▒  ▒███    ▒███  ▒███   ▒▒███ ▒███  █ ▒  ▒███    ▒███
▒███          ▒███████████  ▒███    ▒███ ▒██████    ▒██████████
▒███    █████ ▒███▒▒▒▒▒███  ▒███    ▒███ ▒███▒▒█    ▒███▒▒▒▒▒███
▒▒███  ▒▒███  ▒███    ▒███  ▒███    ███  ▒███ ▒   █ ▒███    ▒███
 ▒▒█████████  █████   █████ ██████████   ██████████ █████   █████
  ▒▒▒▒▒▒▒▒▒  ▒▒▒▒▒   ▒▒▒▒▒ ▒▒▒▒▒▒▒▒▒▒   ▒▒▒▒▒▒▒▒▒▒ ▒▒▒▒▒   ▒▒▒▒▒

```

A lightweight terminal log viewer for Docker containers. The agent runs on your
server beside the Docker daemon and streams parsed, structured logs over QUIC to
a TUI client on your machine.

<img width="1920" height="1031" alt="image" src="../gader/assets/demo_1.png" />



### Simplified Architecture:
```
  ┌──────────────────────────┐                  ┌──────────────────┐
  │  Docker                  │                  │                  │
  │    └──► gader-agent      │ ───── QUIC ───── │  gader (TUI)     │
  │                          │                  │                  │
  └──────────────────────────┘                  └──────────────────┘
         server                                      your machine
```

## Features

- Live log streaming over QUIC with a shared-secret handshake
- Structured log parsing per service (Immich, Vaultwarden)
- Service filter (`Tab`) — pre-filtered server-side to save bandwidth
- Level filter (`l`) — ALL / DEBUG / INFO / LOG / WARN / ERROR
- Live grep search (`s`) — searches message and context fields
- Detail view (`e`) — full message with wrapping
- Scroll + follow mode with `Space` to jump to latest
- Automatic retry when a watched container restarts
- Self-signed TLS cert generated and persisted to `~/.gader/`
- TUI trace logs written to `~/.gader/tui_logs`

## Workspace Layout

```
gader_agent/   — server-side binary: watches Docker, serves clients
gader_tui/     — client-side binary: TUI log viewer
gader_common/  — shared types (LogEntry, NetworkPacket)
```

## Building

Requires Rust nightly (see `rust-toolchain.toml`).

```sh
cargo build --release
```

Binaries will be at:

- `target/release/gader_agent`
- `target/release/gader_tui`

## Setup

### Shared secret

Both the agent and TUI use `~/.gader/` for config. On first run, each creates a
default secret file if one doesn't exist:

| Side   | File                     |
| ------ | ------------------------ |
| Agent  | `~/.gader/server_secret` |
| Client | `~/.gader/client_secret` |

Set both files to the same secret before connecting:

```sh
# on the server
echo "your-secret-here" > ~/.gader/server_secret

# on your machine
echo "your-secret-here" > ~/.gader/client_secret
```

### TLS

On first start, the agent generates a self-signed cert and key:

```
~/.gader/server.cert
~/.gader/server.key
```

These are reused on subsequent starts. Delete them to force regeneration.

> **Note:** The TUI currently skips server certificate verification. TOFU cert pinning is planned.

## Usage

### Agent (server)

```sh
gader-agent [OPTIONS]

Options:
  -l, --listen <ADDR>       Listen address for TUI clients  [default: 0.0.0.0:23456]
  -d, --docker <URL>        Docker HTTP endpoint            [default: http://127.0.0.1:2375]
      --history-size <N>    Log ring buffer size            [default: 150]
      --log-level <LEVEL>   Log level (RUST_LOG takes priority) [default: info]
```

The agent watches the `immich_server` and `vaultwarden` Docker containers by default.

#### Via SSH tunnel (recommended as of now)

Forward Docker's socket over SSH and run the agent locally against it:

```sh
# forward the remote Docker socket to a local port
ssh -N -L 2375:/var/run/docker.sock your-server

# run the agent locally, pointing at the forwarded socket
gader-agent --docker http://127.0.0.1:2375 --listen 127.0.0.1:23456
```

#### Direct (agent on the server)

```sh
# on the server
gader-agent --listen 0.0.0.0:23456

# forward the agent port to your machine
ssh -N -L 23456:localhost:23456 your-server

# on your machine
gader
```

### TUI (client)

```sh
gader [OPTIONS]

Options:
  -s, --server <ADDR>       Agent address to connect to      [default: 127.0.0.1:23456]
      --log-level <LEVEL>   Log level (RUST_LOG takes priority) [default: info]
```

### Keybindings

| Key         | Action                                         |
| ----------- | ---------------------------------------------- |
| `↑` / `↓`   | Navigate log entries                           |
| Scroll      | Navigate log entries                           |
| `Space`     | Jump to latest (re-enable follow mode)         |
| `Tab`       | Cycle service filter                           |
| `l`         | Cycle level filter (All/DEBUG/INFO/WARN/ERROR) |
| `s`         | Open search bar (live grep)                    |
| `Esc`       | Close search / clear query / back              |
| `Enter`     | Close search bar (keep query active)           |
| `e`         | Expand selected entry to detail view           |
| `Backspace` | Back to table from detail view                 |
| `q`         | Quit                                           |

## Adding a New Container / Parser

1. Implement the `LogParser` trait in `gader_agent/src/parsers/`:

```rust
pub struct MyParser;

impl LogParser for MyParser {
    fn parse(&self, line: &str) -> Option<LogEntry> {
        // parse `line`, return Some(LogEntry { .. }) or None
    }
}
```

2. Register it in `gader_agent/src/main.rs` alongside the existing watchers:

```rust
let _task = tokio::spawn(async move {
    spawn_watcher(docker, "my_container", MyParser::new(), tx, c_token, state).await;
});
```

## Configuration Files

All files live in `~/.gader/` on the respective machine.

| File            | Machine | Description                 |
| --------------- | ------- | --------------------------- |
| `server_secret` | Server  | Shared secret (agent side)  |
| `client_secret` | Client  | Shared secret (TUI side)    |
| `server.cert`   | Server  | Self-signed TLS certificate |
| `server.key`    | Server  | TLS private key             |
| `tui_logs`      | Client  | TUI trace log file          |

# His(toré)

![Trust: I read the code](https://img.shields.io/badge/Trust-I_read_the_code-brightgreen)
[![CI](https://github.com/heyjstn/his/actions/workflows/ci.yml/badge.svg)](https://github.com/heyjstn/his/actions/workflows/ci.yml)

His is a terminal application for browsing local coding-agent sessions and reading their message history. Codex and Pi session files are currently supported.

The project is a work in progress.

## Build

His requires a Rust toolchain that supports the Rust 2024 edition.

```sh
cargo build --locked
```

## Configuration

Set `HIS_HOME` to a directory containing `config.toml`:

```sh
export HIS_HOME="$HOME/.his"
mkdir -p "$HIS_HOME"
cp config.example.toml "$HIS_HOME/config.toml"
```

Each configured agent must have a unique `kind`. Environment variables in agent directories are expanded when the configuration is loaded.

```toml
[[agents]]
kind = "codex"
dir = "$HOME/.codex/sessions"

[[agents]]
kind = "pi"
dir = "$HOME/.pi/agent/sessions"
```

Legacy `[[providers]]` entries are not supported.

## Usage

Open the terminal interface:

```sh
cargo run --locked
```

Print session summaries for diagnostics:

```sh
cargo run --locked -- list-session
```

The session list supports typing to filter by working directory, `up` and `down` to browse, `enter` to open, and `esc` or `ctrl+c` to quit. The detail view supports arrow keys or `j` and `k` to scroll, page navigation, `ctrl+o` to toggle commentary, and `esc` to return.

Unreadable session sources are reported as warnings while healthy sessions remain available.

## Development

`dev-test.sh` builds the debug binary, points `HIS_HOME` at the repository's ignored `.his` directory, and forwards arguments to His.

Private histories under `tests/.codex` and `tests/.pi` are ignored. Checked-in parser fixtures belong under `tests/fixtures` and must contain sanitized data only.

Run the same checks used by CI:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
```

## Structure

- `config.rs` loads and resolves configuration.
- `agent.rs` and `agent/` normalize provider-specific session formats.
- `session.rs` defines session summaries, details, messages, and timestamps.
- `repository.rs` discovers sessions, reports source warnings, and loads selected details directly.
- `tui.rs` and `tui/` contain pure application state, terminal effects, and rendering.

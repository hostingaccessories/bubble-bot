# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`bubble-bot` is a Rust CLI tool that spins up ephemeral Docker dev containers pre-configured with Claude Code. It resolves config, renders Dockerfiles from templates, builds content-hash-cached images, starts service containers (MySQL/Postgres/Redis), and execs into the dev container.

## Build & Development Commands

```bash
make build          # cargo build (debug)
make release        # cargo build --release
make test           # cargo test
make lint           # cargo clippy -D warnings + cargo fmt --check
make fmt            # cargo fmt
make check          # cargo check
```

Run a single test: `cargo test <test_name>` (e.g., `cargo test compute_tag_is_deterministic`)

Run the binary: `./target/debug/bubble-bot [command] [flags]`

Enable tracing: `RUST_LOG=info` (or `debug`, `trace`)

## Architecture

**Lifecycle flow:** Config resolution → Dockerfile rendering → Image build (content-hash cached) → Network setup → Service containers → Dev container → Auth injection → Hooks → Exec → Cleanup

**Key abstractions:**
- **`Runtime` trait** (`src/runtime/`): PHP, Node, Rust, Go — each provides `template()` and `template_context()` for MiniJinja Dockerfile rendering
- **`Service` trait** (`src/services/`): MySQL, Postgres, Redis — each provides container config, env vars, readiness commands
- **Manager structs** (`src/docker/`): `ImageBuilder`, `ContainerManager`, `NetworkManager`, `Cleaner` — each wraps a `bollard::Docker` handle and owns lifecycle responsibility
- **`TemplateRenderer`** (`src/templates/`): Combines base + runtime + chief Dockerfile layers using `include_str!` embedded templates

**Config merging (3 layers):** Global `~/.config/bubble-bot/config.toml` → Project `.bubble-bot.toml` → CLI flags

**Image caching:** SHA-256 of rendered Dockerfile → first 12 hex chars → image tag `bubble-bot:<hash>`. Rebuild is skipped if tag exists.

**Cleanup:** `CleanupState` with `Arc<Mutex<...>>` shared between main task and signal handler (SIGINT/SIGTERM).

## Code Conventions

- `anyhow::Result` for all fallible functions; `.context()` for error augmentation; `bail!()` for early returns
- Non-fatal errors (cleanup, hooks) use `warn!()` rather than propagating
- Inline tests: `#[cfg(test)] mod tests` at the bottom of each file
- Manager structs take owned `Docker` (Clone is cheap on bollard::Docker)
- Supported version lists are `const &[&str]` slices validated in `::new()` constructors
- OAuth tokens written via stdin pipe — never exposed in CLI args or env vars

## Key Dependencies

Rust edition 2024, MSRV 1.85. Async via Tokio. Docker API via `bollard`. CLI via `clap` derive. Templates via `minijinja`. Config via `serde` + `toml`.

## Naming Conventions

- Containers: `bubble-bot-<project>`, services: `bubble-bot-<project>-<service>`
- Networks: `bubble-bot-<project>`
- Images: `bubble-bot:<12-char-hash>`
- Volumes: `bubble-bot-<project>-<service>-data`

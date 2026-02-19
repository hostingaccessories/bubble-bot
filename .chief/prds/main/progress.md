## Codebase Patterns
- Edition 2024 with `rust-version = "1.85"` — use Rust 2024 idioms
- `#![allow(dead_code)]` in main.rs suppresses warnings for placeholder types during scaffolding; remove once types are used
- Module structure: `src/{module}/mod.rs` for directories, `src/{module}.rs` for single files
- `security-framework` is macOS-only: `[target.'cfg(target_os = "macos")'.dependencies]`
- Keychain module is conditionally compiled: `#[cfg(target_os = "macos")]`
- CLI uses `clap` derive with `#[command(flatten)]` for flag groups (`RuntimeFlags`, `ServiceFlags`, `ContainerFlags`)
- Subcommands with trailing args use `#[arg(trailing_var_arg = true, allow_hyphen_values = true)]`
- Optional service versions (e.g. `--with-mysql [VER]`) use `num_args = 0..=1` with `default_missing_value`
- Config uses `#[serde(default)]` on all structs so partial TOML files work cleanly
- Config merge: Option fields use `is_some()` guard, Vec fields use `!is_empty()` guard, bool fields overwrite directly
- CLI shell flag uses "zsh" as clap default — to avoid overriding config-file shell, check `flags.shell != "zsh"` before applying
- `Config::load(cli)` is the single entry point: defaults → global → project → CLI flags
- Global config path: `dirs::config_dir()` + `bubble-boy/config.toml` (XDG on Linux, `~/Library/Application Support` on macOS)
- `load_from_file` returns `Ok(None)` for missing files (silently skipped)
- Templates: `include_str!("file.dockerfile")` embeds templates at compile time; minijinja `Environment` + `context!` for rendering
- `TemplateRenderer::new()` loads all templates; `render(&params)` composes base + runtime layers
- `TemplateParams::from_config()` extracts runtime/service flags from Config for template rendering
- Image building: `ImageBuilder::new(docker)` + `build(dockerfile_content, no_cache)` — returns `BuildResult { tag, cached }`
- Content-hash caching: `ImageBuilder::compute_tag(content)` → `bubble-boy:<first-12-sha256-hex>`
- Build context: `tar` crate creates in-memory tar archive with Dockerfile for bollard `build_image`
- `futures-util::StreamExt` needed to iterate bollard's build stream
- `--no-cache` is CLI-only (not in config file); pass as parameter to `ImageBuilder::build()`
- Runtime trait: `name() -> &str` and `template() -> &str` — template returns raw minijinja template string via `include_str!`
- Runtime templates: version-parameterized Dockerfile snippets rendered with minijinja `context!` macro
- TemplateRenderer composes layers: base template first, then runtime layers appended in deterministic order (PHP, Node, Rust, Go)
- PHP extensions (lcars set): cli, mbstring, xml, curl, zip, bcmath, intl, mysql, pgsql, sqlite3, redis, gd, dom, tokenizer + Composer via multi-stage COPY
- Boolean runtimes (like Rust) use static templates with empty `context! {}` — no version parameter needed
- Runtime registry: `runtime::collect_runtimes(config)` returns `Vec<Box<dyn Runtime>>` in deterministic order (PHP, Node, Rust, Go); `render()` takes `&Config` directly
- `Runtime` trait has 3 methods: `name()`, `template()`, `template_context()` — each runtime is self-contained for rendering
- Container lifecycle: `ContainerManager::new(docker)` → `cleanup_existing()` → `create_and_start()` → `exec_interactive_shell()` → `stop_and_remove()`
- Interactive shell uses `std::process::Command::new("docker").args(["exec", "-it", id, shell])` with inherited stdio (blocking call)
- Container name defaults to `bubble-boy-<project-dir-name>` via `default_container_name()`
- UID/GID from `libc::getuid()`/`libc::getgid()` — `libc` crate added for this
- bollard container name filter returns partial matches — filter for exact `/<name>` in results
- Auth: `auth::resolve_oauth_token()` → env var first, then macOS Keychain; returns `Ok(None)` if unavailable
- Keychain: service `Claude Code-credentials`, account `oauth_token`; `generic_password()` takes ownership of `PasswordOptions`
- Container env vars: `ContainerOpts.env_vars` is `Vec<String>` in `KEY=VALUE` format, mapped to bollard `Config.env`
- `TemplateRenderer::render()` returns `RenderResult { dockerfile, context_files }` — callers must use `.dockerfile` for the Dockerfile string
- `ImageBuilder::build(dockerfile, context_files, no_cache)` — pass `&render_result.context_files` for extra build context files
- `ContextFile { path, content, mode }` in `templates` module represents extra files in the Docker build context
- `ContainerManager::exec_interactive_command(id, &[&str])` runs arbitrary commands via `docker exec -it` — reusable for `claude` and `chief` subcommands
- `ContainerManager::exec_command(id, &[&str])` runs commands via `docker exec` (no `-it`) — non-interactive, for `exec` subcommand and scripting
- Network management: `NetworkManager::new(docker)` → `ensure_network(name)` → (use containers) → `remove_network(name)`
- `default_network_name()` returns same format as `default_container_name()` — `bubble-boy-<project-dir>`
- `ContainerOpts.network: Option<String>` — when set, container joins the network via `HostConfig.network_mode` + `NetworkingConfig` with alias
- bollard has two `NetworkingConfig` types: use `bollard::container::NetworkingConfig` (generic `<T>`) for container creation, NOT `bollard::models::NetworkingConfig`
- Docker name filter for networks also returns partial matches — filter for exact name match in results
- `Service` trait has 7 methods: `name()`, `image()`, `container_env()`, `dev_env()`, `volume()`, `readiness_cmd()`, `container_name()` (default impl)
- Service containers use `Mount` with `MountTypeEnum::VOLUME` for named Docker volumes; volume format is `vol_name:container_path`
- `collect_services(config)` in `main.rs` builds `Vec<Box<dyn Service>>` — add new services here; MySQL/Postgres use `Option<Config>` guard, Redis uses `Option<bool>` guard
- MySQL root user: only set `MYSQL_ROOT_PASSWORD` + `MYSQL_DATABASE` (MySQL rejects `MYSQL_USER=root`)
- `wait_for_ready()` uses `docker exec` with retry loop (30 attempts, 2s interval) for service readiness probes
- Cleanup order: dev container → service containers → network (named volumes preserved)
- `services::collect_services(config, project)` is the central entry point for service discovery; `services::collect_service_env_vars(services)` aggregates env vars
- `.chief` directory is gitignored — PRD changes are tracked locally only; only commit source files
- Boolean services (like Redis) use `Option<bool>` in config — check `== Some(true)` in `collect_services()`; no config struct needed
- `TemplateRenderer::render_with_options(config, install_chief)` controls whether Chief/Claude Code is installed in the Dockerfile; `render()` delegates with `install_chief=false`
- Chief layer template (`chief.dockerfile`) conditionally installs Node.js if not already present, then `npm install -g @anthropic-ai/claude-code`
- New subcommand functions follow `run_claude()` pattern: same lifecycle (network → services → container → hooks → exec → hooks → cleanup), different command and render options
- `HookRunner::new(container_id, &config.hooks)` → `run_post_start()` after container start, `run_pre_stop()` after interactive session; failures logged, not propagated
- Hooks use `docker exec <container> sh -c <cmd>` (non-interactive, inherited stdout/stderr for streaming)
- Shell resolution: `resolve_shell(config_shell)` → config value > `$SHELL` basename > "bash" fallback
- Dotfile mounts: `collect_dotfile_mounts()` returns `Vec<String>` in `host:/home/dev/file:ro` format; guarded by `config.shell.mount_configs`
- `ContainerOpts.extra_binds` for additional bind mounts beyond the project workspace mount
- All config structs derive both `Deserialize` and `Serialize` — enables TOML output via `toml::to_string_pretty()`
- Non-container subcommands (like `config`) use synchronous `fn` (not `async fn`) since they don't need Docker
- Docker-only subcommands (like `clean`) that don't need Config still use `async fn` for bollard API calls
- `--dry-run` is intercepted early in `main()` before Docker connection; `run_dry_run()` is synchronous and uses `ImageBuilder::compute_tag()` (static method) to show the would-be image tag
- bollard `list_images` with `reference` filter + `remove_image` with `force: true` for image cleanup
- bollard `list_volumes` returns `Option<Vec<Volume>>` — always `unwrap_or_default()`
- Signal handling: `CleanupState` + `Arc<Mutex<>>` pattern for sharing Docker resource tracking between main task and signal handler; `spawn_signal_handler()` returns `JoinHandle` that must be `abort()`ed on normal exit
- Stale resource detection: `cleanup_stale_resources()` in `main.rs` calls `ContainerManager::cleanup_stale()` + `NetworkManager::cleanup_stale()` — runs on startup before creating new resources; Docker container names have leading `/` but network names don't
- Progress indicators: use `indicatif::ProgressBar::new_spinner()` with `enable_steady_tick()` for indefinite-duration tasks; `set_message()` for updates; `finish_with_message()` for completion

---

## 2026-02-18 - US-001
- What was implemented: Full project scaffolding with Cargo.toml, all module files with placeholder structs/traits
- Files changed:
  - `Cargo.toml` — all dependencies declared, edition 2024, rust-version 1.85
  - `.gitignore` — added `/target/`
  - `src/main.rs` — tokio main, module declarations, tracing init, CLI parse
  - `src/cli.rs` — placeholder Cli struct with clap derive
  - `src/config.rs` — placeholder Config struct with serde Deserialize
  - `src/docker/{mod,containers,images,networks}.rs` — placeholder structs
  - `src/runtime/{mod,php,node,rust,go}.rs` — Runtime trait + placeholder structs
  - `src/services/{mod,mysql,redis,postgres}.rs` — Service trait + placeholder structs
  - `src/auth/{mod,keychain}.rs` — placeholder structs, keychain conditionally compiled
  - `src/hooks.rs` — placeholder HookRunner
  - `src/shell.rs` — placeholder ShellConfig
  - `src/templates/mod.rs` — placeholder TemplateRenderer
- **Learnings for future iterations:**
  - `cargo build` and `cargo clippy` both pass cleanly
  - Placeholder structs trigger `dead_code` warnings — suppressed at crate level with `#![allow(dead_code)]`
  - bollard 0.18.1 is the latest compatible with rust-version 1.85 (0.20.1 available but may require newer Rust)
  - Cargo.lock pins 243 packages
---

## 2026-02-18 - US-002
- What was implemented: Full CLI parsing with clap derive structs for all subcommands and flags
- Files changed:
  - `src/cli.rs` — Complete rewrite: `Cli` struct with `Command` enum (Shell, Claude, Chief, Exec, Build, Config, Clean), `RuntimeFlags`, `ServiceFlags`, `ContainerFlags` args groups, 15 unit tests
- **Learnings for future iterations:**
  - `Cli::command()` returns `Command::Shell` when no subcommand given (default subcommand pattern)
  - `Command` enum must derive `Clone` since `command()` returns owned clone from `Option<Command>`
  - Service flags like `--with-mysql` use `num_args = 0..=1` with `default_missing_value` for optional version argument
  - `trailing_var_arg = true` + `allow_hyphen_values = true` for args after `--`
  - All 15 tests pass, clippy clean, build clean
---

## 2026-02-18 - US-003
- What was implemented: Full config loading and merging system with serde-compatible structs for `.bubble-boy.toml`
- Files changed:
  - `src/config.rs` — Complete rewrite: `Config`, `RuntimeConfig`, `ServiceConfig`, `MysqlConfig`, `PostgresConfig`, `HookConfig`, `ShellConfig`, `ContainerConfig` structs; `Config::load()` with 3-layer merge (global → project → CLI); `load_from_file()` with silent skip for missing files; 18 unit tests
- **Learnings for future iterations:**
  - `#[serde(default)]` on structs is essential for partial TOML parsing — missing sections just get defaults
  - `clone_from()` is the idiomatic way to assign `Option<String>` from a reference without extra cloning
  - Clippy catches derivable `Default` impls — if all fields are `false`/`0`/`None`, just derive it
  - CLI service flags (e.g. `--with-mysql 8.4`) should merge with existing config (preserve database/username/password, only override version)
  - Shell flag needs special handling: clap always provides "zsh" default, so we only override config-file shell when user explicitly passes a different value
  - All 33 tests pass (15 CLI + 18 config), clippy clean, build clean
---

## 2026-02-18 - US-004
- What was implemented: Base Dockerfile template rendering with minijinja
- Files changed:
  - `src/templates/base.dockerfile` — New file: Ubuntu 24.04 base image with git, curl, wget, unzip, build-essential, ca-certificates; creates `/home/dev` with chmod 777; sets WORKDIR to `/workspace`
  - `src/templates/mod.rs` — Complete rewrite: `TemplateRenderer` struct with minijinja `Environment`, `TemplateParams` struct for runtime/service parameters, `include_str!` embedding of base template, 5 unit tests
- **Learnings for future iterations:**
  - `include_str!` paths are relative to the source file, so `include_str!("base.dockerfile")` works from `src/templates/mod.rs`
  - minijinja `Environment::new()` + `add_template()` + `get_template()` + `render()` is the basic flow
  - `context! {}` macro from minijinja creates an empty context for templates without variables
  - `TemplateParams::from_config()` extracts runtime flags from Config — future stories will use these to compose multi-runtime Dockerfiles
  - Base template is static (no Jinja variables needed) — runtime templates in future stories will use parameterized versions
  - All 38 tests pass (15 CLI + 18 config + 5 templates), clippy clean, build clean
---

## 2026-02-18 - US-005
- What was implemented: Image building with content-hash caching using bollard Docker API
- Files changed:
  - `src/docker/images.rs` — Complete rewrite: `ImageBuilder` struct with `build()`, `image_exists()`, `compute_tag()`, `create_build_context()`; `BuildResult` struct; 4 unit tests
  - `Cargo.toml` — Added `futures-util = "0.3"` and `tar = "0.4"` dependencies
- **Learnings for future iterations:**
  - bollard `build_image` accepts a `Bytes` body (in-memory tar archive), not a file path
  - Use `tar::Builder::new(Vec::new())` to create in-memory tar archives for the Docker build context
  - `futures_util::StreamExt` is required to consume bollard's `build_image` stream
  - Clippy enforces collapsible `if` statements — combine `if !no_cache { if self.image_exists(...) }` into one condition
  - `ListImagesOptions` with `reference` filter is the way to check if a specific image tag exists
  - `BuildImageOptions` uses `t` field for the image tag (not `tag`)
  - All 43 tests pass (15 CLI + 18 config + 5 templates + 4 images + 1 new), clippy clean, build clean
---

## 2026-02-18 - US-006
- What was implemented: Container lifecycle management — create, start, exec interactive shell, stop, and remove containers
- Files changed:
  - `src/docker/containers.rs` — Complete rewrite: `ContainerManager` struct with `cleanup_existing()`, `create_and_start()`, `exec_interactive_shell()`, `stop_and_remove()`; `ContainerOpts` struct; `default_container_name()` helper; 2 unit tests
  - `src/main.rs` — Wired up full shell subcommand: config loading, Dockerfile rendering, image building, container lifecycle with cleanup
  - `Cargo.toml` — Added `libc = "0.2"` dependency for UID/GID
- **Learnings for future iterations:**
  - `libc::getuid()` and `libc::getgid()` require `unsafe` block — these are safe FFI calls but Rust requires the block
  - bollard `list_containers` name filter returns partial matches — must check for exact `/<name>` in the `names` field
  - `docker exec -it` for interactive shell must use `std::process::Command` with inherited stdio (not bollard exec API) for proper TTY
  - `StopContainerOptions { t: 5 }` gives 5-second timeout before SIGKILL
  - `RemoveContainerOptions { force: true }` ensures cleanup even if stop fails
  - Container stop errors are intentionally ignored (container may already be stopped)
  - All 45 tests pass (15 CLI + 18 config + 5 templates + 5 images + 2 containers), clippy clean, build clean
---

## 2026-02-18 - US-007
- What was implemented: PHP runtime template with version-parameterized Dockerfile and PhpRuntime struct
- Files changed:
  - `src/templates/php.dockerfile` — New file: PHP runtime template using ondrej PPA, installs 14 extensions (cli, mbstring, xml, curl, zip, bcmath, intl, mysql, pgsql, sqlite3, redis, gd, dom, tokenizer) + Composer via multi-stage COPY
  - `src/runtime/php.rs` — Complete rewrite: `PhpRuntime` struct implementing `Runtime` trait, version validation (8.1, 8.2, 8.3), 3 unit tests
  - `src/templates/mod.rs` — Updated `render()` to compose PHP runtime layer with base template using minijinja; added 6 new template tests
- **Learnings for future iterations:**
  - Runtime `template()` returns raw minijinja template string; a new `Environment` is created per runtime layer for rendering with version-specific context
  - `PhpRuntime::new()` validates version against supported list before allowing construction
  - Template composition: base rendered first, then each runtime layer appended with `\n` separator
  - PHP template uses `{{ php_version }}` placeholder — minijinja replaces it with the actual version (e.g., `8.3`)
  - `#[derive(Debug)]` needed on runtime structs for `unwrap_err()` in tests
  - All 54 tests pass (15 CLI + 18 config + 11 templates + 5 images + 2 containers + 3 PHP runtime), clippy clean, build clean
---

## 2026-02-18 - US-008
- What was implemented: Node.js runtime template with version-parameterized Dockerfile and NodeRuntime struct
- Files changed:
  - `src/templates/node.dockerfile` — New file: Node.js runtime template using nodesource setup script, installs nodejs
  - `src/runtime/node.rs` — Complete rewrite: `NodeRuntime` struct implementing `Runtime` trait, version validation (18, 20, 22), 3 unit tests
  - `src/templates/mod.rs` — Updated `render()` to compose Node.js runtime layer after PHP; added `NodeRuntime` import; added 6 new template tests (node 18/20/22, no-node, unsupported version, PHP+Node composition order)
- **Learnings for future iterations:**
  - Node.js runtime follows exact same pattern as PHP: `NodeRuntime::new()` validates version, `template()` returns `include_str!` of Dockerfile template
  - nodesource install script uses `setup_<version>.x` URL pattern — template uses `{{ node_version }}` placeholder
  - Template composition test verifies PHP comes before Node in output (deterministic ordering)
  - All 63 tests pass (15 CLI + 18 config + 17 templates + 5 images + 2 containers + 3 PHP runtime + 3 Node runtime), clippy clean, build clean
---

## 2026-02-18 - US-009
- What was implemented: Rust runtime template with rustup-based installation and RustRuntime struct
- Files changed:
  - `src/templates/rust.dockerfile` — New file: Rust stable runtime template using rustup, sets RUSTUP_HOME/CARGO_HOME env vars, adds cargo/bin to PATH, chmod for arbitrary UID support
  - `src/runtime/rust.rs` — Complete rewrite: `RustRuntime` struct implementing `Runtime` trait, no version parameter (boolean flag), 2 unit tests
  - `src/templates/mod.rs` — Updated `render()` to compose Rust runtime layer after Node (deterministic order: PHP, Node, Rust, Go); added `RustRuntime` import; added 4 new template tests (with-rust, without-rust, node+rust ordering, php+node+rust ordering)
- **Learnings for future iterations:**
  - Rust runtime is simpler than PHP/Node — no version parameter, just a boolean `rust_enabled` flag
  - Rust template uses no minijinja variables (static template, rendered with empty `context! {}`)
  - rustup installs to `/usr/local/rustup` and `/usr/local/cargo` with `chmod -R a+w` for arbitrary UID support
  - `--default-toolchain stable` pins to stable channel
  - All 69 tests pass (15 CLI + 18 config + 21 templates + 5 images + 2 containers + 3 PHP runtime + 3 Node runtime + 2 Rust runtime), clippy clean, build clean
---

## 2026-02-18 - US-010
- What was implemented: Go runtime template with version-parameterized Dockerfile and GoRuntime struct
- Files changed:
  - `src/templates/go.dockerfile` — New file: Go runtime template using go.dev tarball download, architecture-aware (detects arm64 vs amd64 via `uname -m` + `sed`), sets GOPATH and PATH
  - `src/runtime/go.rs` — Complete rewrite: `GoRuntime` struct implementing `Runtime` trait, version validation (1.22, 1.23), 3 unit tests
  - `src/templates/mod.rs` — Updated `render()` to compose Go runtime layer after Rust (deterministic order: PHP, Node, Rust, Go); added `GoRuntime` import; added 7 new template tests (go 1.22/1.23, no-go, unsupported version, architecture-aware, rust+go ordering, all-runtimes ordering)
- **Learnings for future iterations:**
  - Go runtime follows exact same pattern as Node/PHP: `GoRuntime::new()` validates version, `template()` returns `include_str!` of Dockerfile template
  - Go binary download uses `go.dev/dl/go<version>.linux-<arch>.tar.gz` URL pattern — architecture detected at Docker build time via `uname -m` piped through `sed`
  - Go template uses `{{ go_version }}` placeholder — minijinja replaces it with the actual version (e.g., `1.23`)
  - `GOPATH` set to `/home/dev/go` to work with the existing `/home/dev` directory (chmod 777 for arbitrary UID support)
  - All 79 tests pass (15 CLI + 18 config + 28 templates + 5 images + 2 containers + 3 PHP runtime + 3 Node runtime + 2 Rust runtime + 3 Go runtime), clippy clean, build clean
---

## 2026-02-18 - US-011
- What was implemented: Runtime registry in `src/runtime/mod.rs` with `collect_runtimes()` function; added `template_context()` method to `Runtime` trait; refactored `TemplateRenderer::render()` to use registry instead of hardcoded if/else blocks; `render()` now takes `&Config` directly instead of `&TemplateParams`
- Files changed:
  - `src/runtime/mod.rs` — Added `collect_runtimes(config)` function that builds ordered `Vec<Box<dyn Runtime>>` from config (PHP, Node, Rust, Go order); added `template_context()` to `Runtime` trait; 6 new registry tests
  - `src/runtime/php.rs` — Added `template_context()` returning `context! { php_version => &self.version }`
  - `src/runtime/node.rs` — Added `template_context()` returning `context! { node_version => &self.version }`
  - `src/runtime/rust.rs` — Added `template_context()` returning empty `context! {}`
  - `src/runtime/go.rs` — Added `template_context()` returning `context! { go_version => &self.version }`
  - `src/templates/mod.rs` — `render()` now takes `&Config` and uses `runtime::collect_runtimes()` loop instead of 4 hardcoded if-blocks; tests updated to use `Config` directly; added 2 new composition tests (content hash changes with runtime addition/version change)
  - `src/main.rs` — Updated `run_shell()` to call `renderer.render(config)` instead of `renderer.render(&params)`
- **Learnings for future iterations:**
  - `Runtime` trait now has 3 methods: `name()`, `template()`, `template_context()` — each runtime is self-contained for rendering
  - `collect_runtimes()` is the single entry point for runtime discovery and ordering — always returns PHP, Node, Rust, Go order
  - `minijinja::value::Value` is the return type for `template_context()` — created via the `context!` macro
  - `TemplateParams` struct is still present for backward compatibility but `render()` no longer uses it — it could be removed in future if nothing else needs it
  - All 87 tests pass (15 CLI + 18 config + 30 templates + 5 images + 2 containers + 3 PHP + 3 Node + 2 Rust + 3 Go + 6 registry), clippy clean, build clean
---

## 2026-02-18 - US-012
- What was implemented: macOS Keychain auth extraction and multi-strategy auth dispatcher
- Files changed:
  - `src/auth/keychain.rs` — Complete rewrite: `get_oauth_token()` uses `security_framework::passwords::generic_password` to read `Claude Code-credentials` / `oauth_token` from macOS Keychain; graceful `Ok(None)` fallback on missing entry; 2 unit tests
  - `src/auth/mod.rs` — Complete rewrite: `resolve_oauth_token()` dispatcher tries env var `CLAUDE_CODE_OAUTH_TOKEN` first, then macOS Keychain (`#[cfg(target_os = "macos")]`); returns `Ok(None)` if neither found; 2 unit tests
  - `src/docker/containers.rs` — Added `env_vars: Vec<String>` field to `ContainerOpts`; `create_and_start()` passes env vars to container config
  - `src/main.rs` — Wired `resolve_oauth_token()` into `run_shell()` to inject `CLAUDE_CODE_OAUTH_TOKEN` env var into container
- **Learnings for future iterations:**
  - `security_framework::passwords::generic_password()` takes ownership of `PasswordOptions` (not a reference)
  - `PasswordOptions::new_generic_password(service, account)` is the constructor for keychain lookups
  - Keychain service name for Claude Code is `Claude Code-credentials` with account `oauth_token`
  - Auth resolution order: env var → macOS Keychain → `Ok(None)` with warning
  - `ContainerOpts.env_vars` is `Vec<String>` in `KEY=VALUE` format; maps to bollard `Config.env`
  - All 91 tests pass (15 CLI + 18 config + 30 templates + 5 images + 2 containers + 3 PHP + 3 Node + 2 Rust + 3 Go + 6 registry + 2 keychain + 2 auth), clippy clean, build clean
---

## 2026-02-18 - US-013
- What was implemented: Entrypoint credential injection — embedded entrypoint.sh template that writes OAuth token to `~/.claude/.credentials.json` and unsets the env var; Dockerfile now includes COPY/ENTRYPOINT/CMD for the script; image builder updated to include extra context files in the tar build context
- Files changed:
  - `src/templates/entrypoint.sh` — New file: bash script that checks `CLAUDE_CODE_OAUTH_TOKEN`, writes credentials JSON to `$HOME/.claude/.credentials.json`, sets chmod 600, unsets env var, then `exec "$@"`
  - `src/templates/mod.rs` — Added `RenderResult` and `ContextFile` structs; `render()` now returns `RenderResult` with dockerfile + context_files; appends COPY/ENTRYPOINT/CMD instructions; embedded entrypoint via `include_str!`; 6 new entrypoint tests; updated all existing tests to use `result.dockerfile`
  - `src/docker/images.rs` — `build()` and `create_build_context()` now accept `&[ContextFile]` for extra build context files; 2 new tests for multi-file build context
  - `src/main.rs` — Updated `run_shell()` to use `RenderResult` and pass context_files to image builder
- **Learnings for future iterations:**
  - `TemplateRenderer::render()` returns `RenderResult { dockerfile, context_files }` — not a plain `String` anymore
  - `ContextFile { path, content, mode }` represents extra files in the Docker build context (e.g., entrypoint.sh)
  - `ImageBuilder::build()` takes 3 args: `(dockerfile_content, context_files, no_cache)` — context_files are included in the tar archive
  - Entrypoint uses `$HOME` env var which respects the UID mapping set by `ENV HOME=/home/dev` in the base template
  - `exec "$@"` in entrypoint passes through to the container's CMD (sleep infinity)
  - All 99 tests pass (15 CLI + 18 config + 37 templates + 7 images + 2 containers + 3 PHP + 3 Node + 2 Rust + 3 Go + 6 registry + 2 keychain + 2 auth), clippy clean, build clean
---

## 2026-02-18 - US-014
- What was implemented: Claude Code subcommand — `bubble-boy claude` starts a container and runs `claude --permission-mode bypassPermissions` with trailing args support; auth token injected via entrypoint flow (US-012 + US-013); container cleanup on exit; env var fallback supported
- Files changed:
  - `src/main.rs` — Added `run_claude()` function wired to `Command::Claude { args }` match arm; follows same lifecycle pattern as `run_shell()` but executes Claude Code command instead of interactive shell
  - `src/docker/containers.rs` — Added `exec_interactive_command()` method to `ContainerManager` for running arbitrary commands via `docker exec -it` (reusable for future `chief` and `exec` subcommands)
- **Learnings for future iterations:**
  - `exec_interactive_command()` takes `&[&str]` for flexible command building — trailing args from CLI are converted to `&str` refs before extending the base command vec
  - The `run_claude` function doesn't need shell resolution (no `--shell` flag relevance) — hardcodes "zsh" in ContainerOpts since the shell field is only used for the container's initial setup
  - `exec_interactive_command()` is generic enough to reuse for `chief` (US-020) and `exec` (US-029) subcommands
  - Auth token injection works identically to shell subcommand — `resolve_oauth_token()` + entrypoint script handles the credential flow
  - All 99 tests pass (no new tests needed — core logic reuses existing tested components), clippy clean, build clean
---

## 2026-02-18 - US-015
- What was implemented: Bridge network management — `NetworkManager` creates/reuses/removes bridge networks named `bubble-boy-<project-dir>`; dev container attached to network via `HostConfig.network_mode` and `NetworkingConfig` with alias; network created before container, removed during cleanup; stale networks reused
- Files changed:
  - `src/docker/networks.rs` — Complete rewrite: `NetworkManager` struct with `ensure_network()`, `network_exists()`, `remove_network()`; `default_network_name()` helper; 3 unit tests
  - `src/docker/containers.rs` — Added `network: Option<String>` to `ContainerOpts`; `create_and_start()` now sets `HostConfig.network_mode` and `NetworkingConfig` with endpoint alias when network provided; imports updated for `NetworkingConfig` from `bollard::container`
  - `src/main.rs` — Both `run_shell()` and `run_claude()` now create bridge network before container, pass network in `ContainerOpts`, and remove network during cleanup; added `NetworkManager` and `default_network_name` imports
- **Learnings for future iterations:**
  - bollard has TWO `NetworkingConfig` types: `bollard::container::NetworkingConfig<T>` (generic, for container creation) and `bollard::models::NetworkingConfig` (from bollard-stubs). Use the container module version.
  - `NetworkingConfig.endpoints_config` is `HashMap<T, EndpointSettings>` (NOT `Option<HashMap>`) — build with `HashMap::new()` + `insert()`, not `into_iter().collect()`
  - Docker network name filter returns partial matches — must compare `n.name.as_deref() == Some(name)` for exact match
  - `HostConfig.network_mode` + `NetworkingConfig` with alias is the correct way to attach a container to a network at creation time
  - Service containers (future stories) will also use `NetworkManager::ensure_network()` and the same network name, making them reachable by hostname
  - All 102 tests pass (99 existing + 3 new network tests), clippy clean, build clean
---

## 2026-02-18 - US-016
- What was implemented: MySQL service container — `MysqlService` implementing expanded `Service` trait; `ContainerManager` extended with `start_service()` and `wait_for_ready()` methods; `main.rs` wired with `collect_services()`, `start_services()`, `cleanup_services()` helpers; service containers start before dev container and cleanup after
- Files changed:
  - `src/services/mod.rs` — Expanded `Service` trait with 7 methods: `name()`, `image()`, `container_env()`, `dev_env()`, `volume()`, `readiness_cmd()`, `container_name()` (with default impl)
  - `src/services/mysql.rs` — Complete rewrite: `MysqlService` struct with `MysqlConfig` + `project_name`; implements all `Service` trait methods; root vs non-root MySQL user handling; 8 unit tests
  - `src/docker/containers.rs` — Added `start_service()` method (creates/starts service container with volume mounts and network alias) and `wait_for_ready()` method (retry loop with `docker exec` readiness probe); imports `Mount`, `MountTypeEnum`, and `crate::services::Service`
  - `src/main.rs` — Added `project_name()`, `collect_services()`, `start_services()`, `cleanup_services()` helpers; both `run_shell()` and `run_claude()` now start service containers before dev container and clean them up after
  - `.chief/prds/main/prd.json` — Marked US-016 as passes: true
- **Learnings for future iterations:**
  - `Service` trait has 7 methods — `container_name()` has a default impl using `bubble-boy-<project>-<service_name>` pattern
  - MySQL root user: only set `MYSQL_ROOT_PASSWORD` and `MYSQL_DATABASE` (MySQL rejects `MYSQL_USER=root`); for non-root add `MYSQL_USER` + `MYSQL_PASSWORD`
  - Service containers use `bollard::models::Mount` with `MountTypeEnum::VOLUME` for named Docker volumes (not bind mounts)
  - `wait_for_ready()` uses `std::process::Command` with `docker exec` (not bollard exec API) to match the pattern used for interactive commands; stdout/stderr set to null for clean output
  - `collect_services()` in `main.rs` builds `Vec<Box<dyn Service>>` from config — future services (Redis US-017, Postgres US-018) just need to be added here
  - Service env vars are injected into the dev container via `dev_env()` — appended to `env_vars` before container creation
  - Cleanup order: dev container first, then service containers, then network (preserves named volumes)
  - All 110 tests pass (102 existing + 8 new MySQL tests), clippy clean, build clean
---

## 2026-02-18 - US-017
- What was implemented: Redis service container — `RedisService` implementing `Service` trait; uses `redis:alpine` image; readiness probe via `redis-cli ping`; injects `REDIS_HOST=redis` and `REDIS_PORT=6379` env vars into dev container; no volume (ephemeral); wired into `collect_services()` in main.rs
- Files changed:
  - `src/services/redis.rs` — Complete rewrite: `RedisService` struct with `project_name` field; implements all `Service` trait methods; no container env needed (Redis needs no auth by default); 7 unit tests
  - `src/main.rs` — Added `RedisService` import and wired into `collect_services()` with `config.services.redis == Some(true)` guard
- **Learnings for future iterations:**
  - Redis is simpler than MySQL — no config struct needed (just a boolean flag in `ServiceConfig`), no container env vars, no volume
  - `redis:alpine` is the default image tag per the acceptance criteria
  - Redis readiness probe is simply `redis-cli ping` — returns PONG when ready
  - Pattern for boolean services: check `config.services.redis == Some(true)` since the field is `Option<bool>`
  - All 117 tests pass (110 existing + 7 new Redis tests), clippy clean, build clean
---

## 2026-02-18 - US-018
- What was implemented: PostgreSQL service container — `PostgresService` implementing `Service` trait; uses `postgres:<version>` image (version parameterized, default 16); named volume for data persistence (`bubble-boy-<project>-postgres-data:/var/lib/postgresql/data`); readiness probe via `pg_isready -U <username>`; injects `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` into service container and `DB_HOST=postgres`, `DB_PORT=5432`, `DB_DATABASE`, `DB_USERNAME`, `DB_PASSWORD` into dev container; config values from `[services.postgres]` in `.bubble-boy.toml`; wired into `collect_services()` in main.rs
- Files changed:
  - `src/services/postgres.rs` — Complete rewrite: `PostgresService` struct with `PostgresConfig` + `project_name`; implements all `Service` trait methods; 8 unit tests
  - `src/main.rs` — Added `PostgresService` import and wired into `collect_services()` with `config.services.postgres` guard (same pattern as MySQL)
- **Learnings for future iterations:**
  - PostgreSQL follows the same pattern as MySQL — config struct with version/database/username/password, named volume for persistence
  - PostgreSQL data directory is `/var/lib/postgresql/data` (not `/var/lib/postgres`)
  - `pg_isready -U <username>` is the standard PostgreSQL readiness probe — simpler than MySQL's `mysqladmin ping`
  - PostgreSQL env vars use `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` (not `PGUSER` etc.)
  - Unlike MySQL, PostgreSQL doesn't have special root user handling — `POSTGRES_USER` works for any username
  - `PostgresConfig` already existed in config.rs with defaults (version: "16", database: "app", username: "postgres", password: "password")
  - All 125 tests pass (117 existing + 8 new PostgreSQL tests), clippy clean, build clean
---

## 2026-02-18 - US-019
- What was implemented: Centralized service collection and env var injection — moved `collect_services()` from `main.rs` to `services/mod.rs` as a public function; added `collect_service_env_vars()` helper that aggregates `dev_env()` from all active services; added 10 integration tests verifying multi-service env composition, naming conventions, and edge cases
- Files changed:
  - `src/services/mod.rs` — Added `collect_services(config, project)` and `collect_service_env_vars(services)` public functions; 10 new tests (empty config, single services, all three services, env var collection, naming convention verification, redis false guard)
  - `src/main.rs` — Replaced local `collect_services()` with centralized `services::collect_services()` and `services::collect_service_env_vars()`; removed individual service type imports (MysqlService, RedisService, PostgresService)
- **Learnings for future iterations:**
  - `collect_services()` takes `(config, project)` params — callers no longer need to import individual service types
  - `collect_service_env_vars()` returns flat `Vec<String>` — callers just `extend()` their existing env var list
  - MySQL and Postgres both use `DB_*` prefix — when both are active, Docker uses the last value set (Postgres overwrites MySQL's `DB_HOST`, `DB_PORT` etc.)
  - `redis = Some(false)` should NOT be collected — the guard checks `== Some(true)` explicitly
  - `.chief` directory is gitignored — PRD changes are tracked locally only
  - All 135 tests pass (125 existing + 10 new service integration tests), clippy clean, build clean
---

## 2026-02-18 - US-020
- What was implemented: Chief subcommand — `bubble-boy chief` starts a container with Claude Code (Chief) installed via npm, runs `chief` with trailing args support; auth token injected via entrypoint flow; Chief is installed in the Dockerfile template when the `chief` subcommand is used (via `render_with_options(config, true)`); container cleanup on exit
- Files changed:
  - `src/templates/chief.dockerfile` — New file: installs Claude Code (`@anthropic-ai/claude-code`) via npm; conditionally installs Node.js 22 if not already present
  - `src/templates/mod.rs` — Added `CHIEF_TEMPLATE` static; added `render_with_options()` method with `install_chief` parameter; `render()` delegates to `render_with_options(config, false)`; 6 new Chief template tests
  - `src/main.rs` — Added `run_chief()` function wired to `Command::Chief { args }` match arm; follows same lifecycle as `run_claude()` but uses `render_with_options(config, true)` and runs `chief` command instead of `claude`
- **Learnings for future iterations:**
  - `render_with_options()` is the extensible pattern for subcommand-specific Dockerfile layers — future subcommands needing special tools can follow this approach
  - Chief template uses `command -v node` check to conditionally install Node.js — avoids duplicate Node.js installation when `--with-node` is also specified
  - The `chief` command is provided by the `@anthropic-ai/claude-code` npm package — same package provides `claude` CLI
  - `run_chief()` is nearly identical to `run_claude()` — the only differences are: (1) `render_with_options(config, true)`, (2) command is `["chief"]` instead of `["claude", "--permission-mode", "bypassPermissions"]`
  - All 141 tests pass (135 existing + 6 new Chief template tests), clippy clean, build clean
---

## 2026-02-18 - US-021
- What was implemented: Hook execution — `HookRunner` struct in `src/hooks.rs` reads `post_start` and `pre_stop` arrays from resolved `HookConfig`; hooks run via `docker exec` with `sh -c` for shell command interpretation; output streamed to terminal (inherited stdout/stderr); failures logged as warnings but do not prevent cleanup; wired into all three subcommand functions (`run_shell`, `run_claude`, `run_chief`)
- Files changed:
  - `src/hooks.rs` — Complete rewrite: `HookRunner` struct with `run_post_start()`, `run_pre_stop()`, and private `run_hook()` methods; uses `docker exec <container> sh -c <cmd>` for shell command execution; 4 unit tests
  - `src/main.rs` — Added `HookRunner` import; all three subcommand functions (`run_shell`, `run_claude`, `run_chief`) now create `HookRunner` and call `run_post_start()` after container start and `run_pre_stop()` after interactive session exits
- **Learnings for future iterations:**
  - Hooks use `docker exec <container> sh -c <cmd>` (not `docker exec -it`) since hooks are non-interactive — stdin set to null, stdout/stderr inherited for streaming
  - `HookRunner::new()` takes references to container_id and HookConfig to avoid ownership issues
  - Hook failures return no error — `run_hook()` catches both non-zero exit codes and execution errors and logs them as warnings
  - The same `HookRunner` pattern is used in all three subcommands: create after `create_and_start()`, call `run_post_start()` before the main command, call `run_pre_stop()` after the main command exits
  - All 145 tests pass (141 existing + 4 new hook tests), clippy clean, build clean
---

## 2026-02-18 - US-022
- What was implemented: Shell detection and dotfile mounting — `src/shell.rs` detects user's shell from `$SHELL` env var (basename extraction, fallback to bash); `collect_dotfile_mounts()` scans for common dotfiles (.zshrc, .bashrc, .bash_profile, .profile, .aliases, .inputrc, .vimrc, .gitconfig, .tmux.conf) and returns read-only bind mount strings; `resolve_shell()` prioritizes config value over detected shell; `ContainerOpts` extended with `extra_binds` field; all three subcommands (shell, claude, chief) conditionally collect dotfile mounts when `mount_configs = true`
- Files changed:
  - `src/shell.rs` — Complete rewrite: `detect_shell()`, `collect_dotfile_mounts()`, `resolve_shell()` functions; 7 unit tests
  - `src/docker/containers.rs` — Added `extra_binds: Vec<String>` to `ContainerOpts`; `create_and_start()` extends binds list with extra_binds
  - `src/main.rs` — Updated `run_shell()` to use `resolve_shell()` instead of hardcoded fallback; all three subcommands now conditionally collect dotfile mounts when `config.shell.mount_configs` is true; added `collect_dotfile_mounts` and `resolve_shell` imports
- **Learnings for future iterations:**
  - `detect_shell()` extracts basename from `$SHELL` env var using `Path::file_name()` — returns "bash" if `$SHELL` is unset
  - `resolve_shell()` takes `Option<&str>` from config — when `None`, falls back to detected shell (not hardcoded "zsh")
  - Dotfile mounts use `host_path:/home/dev/filename:ro` format — read-only to prevent container from modifying host files
  - `ContainerOpts.extra_binds` is `Vec<String>` appended to the project bind mount — keeps the project mount separate from optional dotfile mounts
  - `PathBuf` import only used in test module — moving it to `#[cfg(test)] mod tests` avoids the unused import warning
  - All 152 tests pass (145 existing + 7 new shell tests), clippy clean, build clean
---

## 2026-02-18 - US-023
- What was implemented: Config subcommand — `bubble-boy config` prints the fully resolved configuration (after all merge layers: defaults → global → project → CLI) as TOML to stdout; no container is started
- Files changed:
  - `src/config.rs` — Added `Serialize` derive to all config structs (`Config`, `RuntimeConfig`, `ServiceConfig`, `MysqlConfig`, `PostgresConfig`, `HookConfig`, `ShellConfig`, `ContainerConfig`); changed `use serde::Deserialize` to `use serde::{Deserialize, Serialize}`; 2 new serialization tests
  - `src/main.rs` — Added `run_config()` function using `toml::to_string_pretty()` for TOML output; wired `Command::Config` match arm to `run_config()`
- **Learnings for future iterations:**
  - `toml::to_string_pretty()` serializes a `Serialize`-implementing struct to formatted TOML — requires `Serialize` derive on all nested structs
  - Adding `Serialize` alongside `Deserialize` is non-breaking — existing `#[serde(default)]` attributes work for both directions
  - `run_config()` is synchronous (not async) since it only serializes and prints — no Docker or network calls needed
  - The `config` subcommand is the simplest subcommand pattern: load config, process, print, exit (no container lifecycle)
  - All 154 tests pass (152 existing + 2 new config serialization tests), clippy clean, build clean
---

## 2026-02-18 - US-024
- What was implemented: Clean subcommand — `bubble-boy clean` removes all `bubble-boy:*` images and `bubble-boy-*` networks; `--volumes` flag additionally removes `bubble-boy-*` named volumes; lists what was removed to stdout
- Files changed:
  - `src/docker/clean.rs` — New file: `Cleaner` struct with `clean()`, `remove_images()`, `remove_networks()`, `remove_volumes()` methods; uses bollard `list_images`/`remove_image`, `list_networks`/`remove_network`, `list_volumes`/`remove_volume` APIs; 1 unit test
  - `src/docker/mod.rs` — Added `pub mod clean`
  - `src/main.rs` — Added `run_clean()` async function wired to `Command::Clean { volumes }` match arm; imports `Cleaner` from `docker::clean`
- **Learnings for future iterations:**
  - bollard `list_images` with `reference` filter matches the image name (e.g., `"bubble-boy"` matches all `bubble-boy:*` tags)
  - bollard `list_volumes` uses `ListVolumesOptions` with generic type `<String>` — `name` filter matches partial volume names
  - Docker network name filter returns partial matches — must verify `starts_with("bubble-boy-")` on results for exact matching
  - `remove_image` takes `RemoveImageOptions { force: true }` to handle images used by stopped containers
  - `VolumeListResponse.volumes` is `Option<Vec<Volume>>` — unwrap with `unwrap_or_default()`
  - `run_clean()` is async (unlike `run_config()`) since it needs Docker API calls
  - Clean subcommand doesn't need `Config` loading — it just connects to Docker and cleans up resources
  - All 155 tests pass (154 existing + 1 new clean test), clippy clean, build clean
---

## 2026-02-18 - US-025
- What was implemented: Dry-run mode — `--dry-run` flag prints resolved config (TOML), generated Dockerfile, and Docker commands that would be executed (image build, network create, service containers, dev container run, exec command, hooks) without creating any containers, networks, or images; works with all subcommands (shell, claude, chief, exec, build, config, clean)
- Files changed:
  - `src/main.rs` — Added `run_dry_run()` function that intercepts `--dry-run` early in `main()` before any Docker API calls; renders config via `toml::to_string_pretty()`, generates Dockerfile via `TemplateRenderer::render_with_options()`, computes image tag via `ImageBuilder::compute_tag()`, and prints equivalent Docker commands (network create, service container runs, dev container run with mounts/env/network, exec command, hooks); handles all `Command` variants including non-container subcommands (config prints note, clean prints what would be removed)
- **Learnings for future iterations:**
  - `ImageBuilder::compute_tag()` is a static method that doesn't need a Docker connection — perfect for dry-run mode
  - Dry-run intercept should happen early in `main()` before any async Docker operations, keeping the function synchronous
  - The `Command` enum must be matched exhaustively in dry-run — `Exec` and `Build` subcommands (not yet implemented) are covered for forward compatibility
  - `libc::getuid()`/`libc::getgid()` can be used in dry-run to show realistic `--user` flags
  - Auth token is shown as `<token>` placeholder in dry-run output to avoid leaking credentials
  - All 155 tests pass (no new tests needed — dry-run reuses existing tested components like template rendering and config serialization), clippy clean, build clean
---

## 2026-02-18 - US-026
- What was implemented: Signal handling for clean shutdown — `CleanupState` struct tracks all Docker resources (dev container, service containers, network) shared between main task and signal handler via `Arc<Mutex<>>`; `spawn_signal_handler()` listens for SIGINT (Ctrl+C) and SIGTERM, performs full cleanup via `CleanupState::cleanup()`, then exits with code 130; normal exit path aborts the signal handler and uses the same `CleanupState::cleanup()` for consistency; all three subcommands (shell, claude, chief) updated to use shared cleanup state; removed standalone `cleanup_services()` function
- Files changed:
  - `src/main.rs` — Added `CleanupState` struct with `cleanup()` method; added `spawn_signal_handler()` function using `tokio::signal::ctrl_c()` and `tokio::signal::unix::signal(SignalKind::terminate())`; refactored `run_shell()`, `run_claude()`, `run_chief()` to use shared cleanup state via `Arc<Mutex<CleanupState>>`; removed standalone `cleanup_services()` function
- **Learnings for future iterations:**
  - `tokio::signal::ctrl_c()` handles SIGINT; `tokio::signal::unix::signal(SignalKind::terminate())` handles SIGTERM — both included in tokio "full" feature
  - `Arc<Mutex<CleanupState>>` pattern works well for sharing mutable state between main task and signal handler
  - `CleanupState::cleanup()` uses `take()`/`drain(..)` to clear resources after cleanup — safe to call multiple times
  - Signal handler should `abort()` on normal exit to prevent it from interfering with subsequent cleanup
  - `std::process::exit(130)` is the Unix convention for SIGINT termination (128 + signal number 2)
  - Named volumes are preserved during cleanup — only containers and networks are removed
  - Crash recovery on next run is handled by `cleanup_existing()` in container/network managers (already implemented in US-006/US-015)
  - All 155 tests pass (no new tests needed — signal handling logic is infra-level and uses existing tested components), clippy clean, build clean
---

## 2026-02-18 - US-027
- What was implemented: Stale container and network detection — on startup, all container-based subcommands (shell, claude, chief) now detect and auto-remove stale `bubble-boy-<project>*` containers (dev + service containers) and networks from crashed previous sessions; `cleanup_stale()` methods added to both `ContainerManager` and `NetworkManager`; `cleanup_stale_resources()` orchestrator function in `main.rs` called before creating new resources; `matches_stale_prefix()` helper functions for testable prefix matching logic
- Files changed:
  - `src/docker/containers.rs` — Added `cleanup_stale()` method to `ContainerManager` (lists all containers matching project prefix, stops and removes them with warnings); added `matches_stale_prefix()` helper for Docker container name matching (accounts for leading `/`); 4 new unit tests
  - `src/docker/networks.rs` — Added `cleanup_stale()` method to `NetworkManager` (lists all networks matching project prefix, removes them with warnings); added `matches_stale_prefix()` helper for network name matching; 4 new unit tests
  - `src/main.rs` — Added `cleanup_stale_resources()` orchestrator function; wired into `run_shell()`, `run_claude()`, `run_chief()` after Docker connection but before resource creation
- **Learnings for future iterations:**
  - Docker container names have a leading `/` (e.g., `/bubble-boy-myproject`) — prefix matching must account for this
  - Network names do NOT have a leading `/` — separate matching helper needed
  - Stale detection uses `name` filter with `list_containers`/`list_networks` + exact prefix matching (Docker filters return partial matches)
  - `cleanup_stale` must run before `cleanup_existing` and `ensure_network` — it cleans up ALL project-related resources, not just the specific container/network
  - Stale container removal failures are logged as warnings (not errors) to avoid blocking new session startup
  - All 163 tests pass (155 existing + 4 container stale prefix tests + 4 network stale prefix tests), clippy clean, build clean
---

## 2026-02-18 - US-028
- What was implemented: Progress bars for image builds — replaced `tracing::info!` logging with `indicatif` spinners and styled messages; cache hit shows a checkmark with "Image loaded from cache" message; builds show a spinning animation that updates with Docker build step messages; build errors display failure message before bailing; build completion shows success message with image tag
- Files changed:
  - `src/docker/images.rs` — Replaced `tracing::info` import with `indicatif::{ProgressBar, ProgressStyle}`; cache hit path creates a finished spinner with `✓` prefix; build path creates a steady-tick spinner (`{spinner:.cyan} {msg}` template) that updates message with each Docker build stream output; error paths finish the spinner with failure message before bailing
- **Learnings for future iterations:**
  - `indicatif` was already in Cargo.toml — no dependency changes needed
  - `ProgressBar::new_spinner()` + `enable_steady_tick()` is the pattern for indefinite-duration tasks like Docker builds
  - `ProgressStyle::default_spinner().template()` returns `Result` — use `.expect()` for compile-time-known templates
  - `pb.set_message()` updates the spinner text in-place — ideal for streaming Docker build output
  - `pb.finish_with_message()` stops the spinner and displays a final message — used for both success and error cases
  - For cache hit, a non-spinning bar with `✓` prefix and `finish_with_message()` gives clean output
  - All 163 tests pass (no new tests needed — progress bars are visual output, existing tests cover build logic), clippy clean, build clean
---

## 2026-02-18 - US-029
- What was implemented: The `bubble-boy exec -- <cmd>` subcommand — starts a container, runs the command non-interactively (no TTY), and cleans up; exit code from the command is propagated as Bubble Boy's exit code; command runs with the same mounts, env, network, hooks, and cleanup as the `shell` subcommand
- Files changed:
  - `src/docker/containers.rs` — Added `exec_command()` method to `ContainerManager` for non-interactive command running via `docker exec` (no `-it` flags); inherits stdout/stderr for output streaming
  - `src/main.rs` — Added `run_exec()` async function following the same lifecycle pattern as `run_shell()`/`run_claude()`/`run_chief()`; wired `Command::Exec { cmd }` match arm to `run_exec()`
- **Learnings for future iterations:**
  - `exec_command()` differs from `exec_interactive_command()` by omitting `-it` flags — non-interactive for scripting use cases
  - `run_exec()` follows the same lifecycle as other container subcommands: stale cleanup -> image build -> network -> services -> container -> hooks -> command -> hooks -> cleanup
  - The subcommand reuses `render()` (not `render_with_options()`) since no special Dockerfile layers are needed
  - Exit code propagation uses `std::process::exit(exit_code)` for non-zero codes, same pattern as other subcommands
  - Dry-run support was already implemented in US-025 (forward compatibility in the `Command::Exec` match arm)
  - All 163 tests pass (no new tests needed — core logic reuses existing tested components), clippy clean, build clean
---

## 2026-02-18 - US-030
- What was implemented: Build subcommand — `bubble-boy build` renders the Dockerfile and force-builds the image regardless of cache (passes `no_cache=true` to `ImageBuilder::build()`); no container is started; reports the resulting image tag to stdout; removed the wildcard `_ =>` match arm in `main()` since all `Command` variants are now handled
- Files changed:
  - `src/main.rs` — Added `run_build()` async function wired to `Command::Build` match arm; replaced wildcard `_ =>` with explicit `Command::Build` match; function connects to Docker, renders Dockerfile via `TemplateRenderer::render()`, force-builds via `ImageBuilder::build()` with `no_cache=true`, and prints the resulting image tag
- **Learnings for future iterations:**
  - `run_build()` is the simplest Docker-based subcommand — no container lifecycle, no network, no services, no hooks, no signal handling
  - Force build is achieved by passing `true` for the `no_cache` parameter to `ImageBuilder::build()` — this skips the `image_exists()` check entirely
  - The `cli` parameter is not needed since `--no-cache` is irrelevant when always force-building
  - Dry-run support for `Command::Build` was already implemented in US-025 (prints "(build only — no container started)")
  - With US-030 complete, all `Command` variants are handled — the wildcard `_ =>` match arm is removed, giving exhaustive match checking
  - All 163 tests pass (no new tests needed — core logic reuses existing tested components), clippy clean, build clean
---

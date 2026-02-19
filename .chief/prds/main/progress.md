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

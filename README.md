# bubble-bot

Ephemeral Docker dev containers pre-configured with [Claude Code](https://docs.anthropic.com/en/docs/claude-code).

bubble-bot resolves config, renders Dockerfiles from templates, builds
content-hash-cached images, starts service containers (MySQL, PostgreSQL,
Redis), and drops you into an interactive dev container with Claude Code
ready to go.

## Requirements

- Docker (running locally)
- Rust 1.85+ (to build from source)
- macOS (for Keychain-based OAuth token resolution; manual token export works
  on any platform)

## Installation

```bash
git clone https://github.com/yourorg/bubble-bot.git
cd bubble-bot
make release
cp target/release/bubble-bot /usr/local/bin/
```

## Quick Start

```bash
# Open an interactive shell in a dev container
cd my-project
bubble-bot

# Run Claude Code inside the container
bubble-bot claude

# Run Claude Code with extra arguments
bubble-bot claude -- --model sonnet -p "fix the tests"

# Run a one-off command
bubble-bot exec -- cargo test

# Build the image without starting a container
bubble-bot build

# Print resolved configuration
bubble-bot config

# Clean up all bubble-bot images and networks
bubble-bot clean
bubble-bot clean --volumes  # also remove data volumes
```

## Commands

| Command | Description |
|---------|-------------|
| `shell` | Open an interactive shell (default when no command is given) |
| `claude [-- ARGS...]` | Run Claude Code with `--permission-mode bypassPermissions` |
| `chief [-- ARGS...]` | Run Chief (autonomous Claude Code task runner) |
| `exec CMD [ARGS...]` | Run a command in the container and exit |
| `build` | Build the container image (always forces rebuild) |
| `config` | Print resolved config as TOML to stdout |
| `clean [--volumes]` | Remove all bubble-bot images, networks, and optionally volumes |

## Flags

### Runtime Flags

| Flag | Description |
|------|-------------|
| `--with-php VERSION` | Include PHP (8.1, 8.2, or 8.3) |
| `--with-node VERSION` | Include Node.js (18, 20, or 22) |
| `--with-rust` | Include Rust stable toolchain |
| `--with-go VERSION` | Include Go (1.22 or 1.23) |

### Service Flags

| Flag | Description |
|------|-------------|
| `--with-mysql [VERSION]` | Start MySQL (default: 8.0) |
| `--with-postgres [VERSION]` | Start PostgreSQL (default: 16) |
| `--with-redis` | Start Redis |

### Container Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--name NAME` | `bubble-bot-<dir>` | Container name |
| `--network NAME` | `bubble-bot-<dir>` | Docker network name |
| `--shell SHELL` | `bash` | Shell to use inside the container |
| `--no-cache` | | Force image rebuild, ignore cache |
| `--dry-run` | | Print what would be done without executing |

## Configuration

Configuration merges three layers (lowest to highest precedence):

1. **Global** `~/.config/bubble-bot/config.toml`
2. **Project** `.bubble-bot.toml` in the current directory
3. **CLI flags**

### Example `.bubble-bot.toml`

```toml
[runtimes]
php = "8.3"
node = "22"

[services.mysql]
version  = "8.0"
database = "myapp"
username = "root"
password = "secret"

[services]
redis = true

[services.postgres]
version  = "16"
database = "myapp"
username = "postgres"
password = "secret"

[hooks]
post_start = ["composer install", "npm ci", "php artisan migrate"]
pre_stop   = ["php artisan queue:restart"]

[container]
name    = "my-container"
network = "my-network"
shell   = "zsh"
```

### Config Reference

#### `[runtimes]`

| Key | Type | Values |
|-----|------|--------|
| `php` | string | `"8.1"`, `"8.2"`, `"8.3"` |
| `node` | string | `"18"`, `"20"`, `"22"` |
| `rust` | bool | `true` |
| `go` | string | `"1.22"`, `"1.23"` |

#### `[services.mysql]`

| Key | Type | Default |
|-----|------|---------|
| `version` | string | `"8.0"` |
| `database` | string | `"app"` |
| `username` | string | `"root"` |
| `password` | string | `"password"` |

#### `[services.postgres]`

| Key | Type | Default |
|-----|------|---------|
| `version` | string | `"16"` |
| `database` | string | `"app"` |
| `username` | string | `"postgres"` |
| `password` | string | `"password"` |

#### `[services]`

| Key | Type | Default |
|-----|------|---------|
| `redis` | bool | `false` |

#### `[hooks]`

| Key | Type | Description |
|-----|------|-------------|
| `post_start` | string[] | Commands to run after container starts, before the main command |
| `pre_stop` | string[] | Commands to run after the main command exits, before cleanup |

Hooks run sequentially inside the container via `sh -c`. Failures are
logged as warnings but do not abort execution.

#### `[container]`

| Key | Type | Default |
|-----|------|---------|
| `name` | string | `bubble-bot-<dir>` |
| `network` | string | `bubble-bot-<dir>` |
| `shell` | string | `bash` |

## Authentication

bubble-bot injects Claude Code credentials into the container
automatically. OAuth tokens are resolved in order:

1. `CLAUDE_CODE_OAUTH_TOKEN` environment variable
2. macOS Keychain (`Claude Code-credentials` service)

If neither is found, a warning is logged and Claude Code may fail to
authenticate.

Credentials are written into the container via stdin pipe — they are never
exposed in CLI arguments or environment variables.

## Environment Variables

### Host

| Variable | Description |
|----------|-------------|
| `CLAUDE_CODE_OAUTH_TOKEN` | OAuth token for Claude Code |
| `RUST_LOG` | Log level (`info`, `debug`, `trace`) |

### Injected into Dev Container

| Variable | Source | Value |
|----------|--------|-------|
| `DB_HOST` | MySQL / Postgres | `mysql` or `postgres` |
| `DB_PORT` | MySQL / Postgres | `3306` or `5432` |
| `DB_DATABASE` | MySQL / Postgres | configured database name |
| `DB_USERNAME` | MySQL / Postgres | configured username |
| `DB_PASSWORD` | MySQL / Postgres | configured password |
| `REDIS_HOST` | Redis | `redis` |
| `REDIS_PORT` | Redis | `6379` |

## Image Caching

Rendered Dockerfiles are SHA-256 hashed. The first 12 hex characters form
the image tag (`bubble-bot:<hash>`). If a matching image exists locally,
the build is skipped. Use `--no-cache` to force a rebuild.

## Naming Conventions

| Resource | Pattern | Example |
|----------|---------|---------|
| Dev container | `bubble-bot-<project>` | `bubble-bot-myapp` |
| Service container | `bubble-bot-<project>-<service>` | `bubble-bot-myapp-mysql` |
| Network | `bubble-bot-<project>` | `bubble-bot-myapp` |
| Image | `bubble-bot:<hash>` | `bubble-bot:a1b2c3d4e5f6` |
| Volume | `bubble-bot-<project>-<service>-data` | `bubble-bot-myapp-mysql-data` |

## Lifecycle

1. Connect to Docker
2. Clean up stale resources from prior sessions
3. Render Dockerfile (base + runtimes + optional chief layer)
4. Build image (or load from cache)
5. Resolve OAuth token and Claude config
6. Create bridge network
7. Start service containers and wait for readiness
8. Start dev container (runs as your UID/GID, mounts project at `/workspace`)
9. Write credentials into container
10. Run `post_start` hooks
11. Execute main command (shell, claude, chief, or exec)
12. Run `pre_stop` hooks
13. Clean up containers and network

Signal handlers (SIGINT, SIGTERM) ensure cleanup runs even on interruption.

## Development

```bash
make build     # cargo build (debug)
make release   # cargo build --release
make test      # cargo test
make lint      # cargo clippy -D warnings + cargo fmt --check
make fmt       # cargo fmt
make check     # cargo check
```

Run a single test:

```bash
cargo test compute_tag_is_deterministic
```

Enable tracing:

```bash
RUST_LOG=debug bubble-bot
```

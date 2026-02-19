#!/bin/bash
set -e

# Write Claude Code OAuth token to credentials file if present
if [ -n "${CLAUDE_CODE_OAUTH_TOKEN}" ]; then
    mkdir -p "${HOME}/.claude"
    cat > "${HOME}/.claude/.credentials.json" <<CREDENTIALS
{
  "claudeAiOauth": {
    "token": "${CLAUDE_CODE_OAUTH_TOKEN}"
  }
}
CREDENTIALS
    chmod 600 "${HOME}/.claude/.credentials.json"
    unset CLAUDE_CODE_OAUTH_TOKEN
fi

# Execute the original command
exec "$@"

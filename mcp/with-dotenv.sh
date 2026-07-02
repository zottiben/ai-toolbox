#!/usr/bin/env bash
# with-dotenv.sh — load a project .env into the environment, then exec a command.
# Lets MCP servers (and any CLI) read secrets from a root .env WITHOUT you having
# to `source` it in your shell first, and supports multiple environments.
#
# In .mcp.json (copy this script to <repo>/.claude/mcp/with-dotenv.sh):
#   "command": ".claude/mcp/with-dotenv.sh",
#   "args": ["--", "npx", "-y", "@supabase/mcp-server-supabase@latest", "--read-only", "--project-ref=<ref>"]
#
# Multiple environments from ONE root .env — remap a differently-named var onto the
# name the server expects, with --set TARGET=SOURCE (repeatable):
#   "args": ["--set", "SUPABASE_ACCESS_TOKEN=SUPABASE_STAGING_TOKEN", "--", "npx", …]
#
# Point at a different file with --env-file <path> (e.g. .env.staging).
# Assumes a simple KEY=value .env (the shell sources it).
set -euo pipefail

ENV_FILE=""
REMAPS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --env-file) ENV_FILE="${2:-}"; shift 2 ;;
    --set)      REMAPS+=("${2:-}"); shift 2 ;;
    --)         shift; break ;;
    *)          break ;;
  esac
done
[ -n "$ENV_FILE" ] || ENV_FILE="${CLAUDE_PROJECT_DIR:-.}/.env"

if [ -f "$ENV_FILE" ]; then
  set -a; . "$ENV_FILE"; set +a
fi

if [ "${#REMAPS[@]}" -gt 0 ]; then
  for m in "${REMAPS[@]}"; do
    tgt="${m%%=*}"; src="${m#*=}"
    export "$tgt"="${!src:-}"
  done
fi

exec "$@"

#!/usr/bin/env bash
# dotenv-header.sh — emit MCP auth headers as JSON, reading the token from a repo .env.
#
# For a REMOTE HTTP MCP whose token lives only in the repo's .env (never exported), a
# static "Authorization: Bearer ${VAR}" header can't work: Claude Code resolves ${VAR}
# from its OWN launch environment and *fails to parse the config* when it's unset. Use
# this as the server's `headersHelper` instead — Claude Code runs it at connection time
# and merges its JSON stdout into the request headers, so the token is read from .env
# then, not at launch. (with-dotenv.sh solves the same problem for stdio servers, by
# wrapping the process; an HTTP server has no local process to wrap.)
#
# In .mcp.json (copy this to <repo>/.claude/mcp/dotenv-header.sh):
#   "type": "http",
#   "url": "https://api.example.com/mcp",
#   "headersHelper": ".claude/mcp/dotenv-header.sh EXAMPLE_API_TOKEN"
# emits:  {"Authorization":"Bearer <EXAMPLE_API_TOKEN from .env>"}
#
# Options (before the VAR name):
#   --env-file <path>   read a file other than ./.env
#   --header <name>     header name (default: Authorization)
#   --scheme <scheme>   prefix before the value (default: Bearer; pass "" for none)
set -euo pipefail

ENV_FILE=""; HEADER="Authorization"; SCHEME="Bearer"
while [ $# -gt 0 ]; do
  case "$1" in
    --env-file) ENV_FILE="${2:-}"; shift 2 ;;
    --header)   HEADER="${2:-}"; shift 2 ;;
    --scheme)   SCHEME="${2:-}"; shift 2 ;;
    --)         shift; break ;;
    -*)         echo "dotenv-header: unknown option $1" >&2; exit 2 ;;
    *)          break ;;
  esac
done

VAR="${1:-}"
[ -n "$VAR" ] || { echo "dotenv-header: usage: dotenv-header.sh [--header H] [--scheme S] VAR_NAME" >&2; exit 2; }

# headersHelper runs with cwd = the session's working directory (repo root); CLAUDE_PROJECT_DIR
# may be unset in that environment, so fall back to ./.env.
[ -n "$ENV_FILE" ] || ENV_FILE="${CLAUDE_PROJECT_DIR:-.}/.env"
[ -f "$ENV_FILE" ] && { set -a; . "$ENV_FILE"; set +a; }

VAL="${!VAR:-}"
[ -n "$VAL" ] || { echo "dotenv-header: '$VAR' is empty (not found in $ENV_FILE)" >&2; exit 1; }
[ -n "$SCHEME" ] && VAL="$SCHEME $VAL"

# JSON-escape backslash and double-quote, then emit a single-object headers map on stdout.
esc=${VAL//\\/\\\\}; esc=${esc//\"/\\\"}
printf '{"%s":"%s"}\n' "$HEADER" "$esc"

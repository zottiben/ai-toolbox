#!/usr/bin/env bash
# setup.sh — the guided walkthrough behind `ai-toolbox setup`, sourced by bin/ai-toolbox.
#
# `bootstrap` is the fast path (detect a stack, confirm four groups, install). This is the
# thorough one: it also picks the harnesses, installs their prerequisites, scaffolds the
# knowledge files, fills in the MCP secrets, and ends by telling you exactly how to start
# each harness. It interviews first and writes nothing until you approve one plan, so
# quitting halfway leaves the repo untouched.
#
# It composes the same cmd_* subcommands the rest of the CLI exposes — no separate install
# path to drift. UI widgets come from bin/lib/ui.sh (gum when present, plain prompts when not).
# Like the rest of the toolkit this stays bash 3.2 clean (macOS's /bin/bash): no namerefs,
# no mapfile, and every possibly-empty array expansion guarded for `set -u`.
#
# shellcheck shell=bash
# shellcheck disable=SC2154,SC2034  # output helpers and $HARNESS belong to bin/ai-toolbox

# ----------------------------------------------------------------------------
# harness knowledge — what each one is called, how to detect it, how to start it
# ----------------------------------------------------------------------------
_setup_harness_label() {
  case "$1" in
    claude) printf 'claude  (Claude Code — CLAUDE.md + pointers into .agents/)' ;;
    codex)  printf 'codex   (Codex CLI — reads AGENTS.md + .agents/ natively)' ;;
    pi)     printf 'pi      (Pi — reads AGENTS.md + .agents/ + .mcp.json natively)' ;;
  esac
}

# Installed (CLI on PATH) or already used here (config dir present).
_setup_harness_present() {
  case "$1" in
    claude) have claude || [ -d "$HOME/.claude" ] || [ -d "$REPO/.claude" ] || [ -f "$REPO/.mcp.json" ] ;;
    codex)  have codex  || [ -d "$HOME/.codex" ]  || [ -d "$REPO/.codex" ] ;;
    pi)     have pi     || [ -d "$HOME/.pi" ]     || [ -d "$REPO/.pi" ] ;;
  esac
}

_setup_charter_path() {
  case "$1" in
    claude) printf '%s' "$HOME/.claude/CLAUDE.md" ;;
    codex)  printf '%s' "$HOME/.codex/AGENTS.md" ;;
    pi)     printf '%s' "$(_pi_agent_dir)/AGENTS.md" ;;
  esac
}

# ----------------------------------------------------------------------------
# collected answers (filled by the interview, consumed by the apply phase)
# ----------------------------------------------------------------------------
SETUP_HARNESSES=(); SETUP_MCP=(); SETUP_HOOKS=(); SETUP_SKILLS=(); SETUP_RULES=()
SETUP_SCAFFOLD=0; SETUP_CHARTER=0; SETUP_PI_INIT=0; SETUP_APPEND_RULES=0; SETUP_MIGRATE=0
SETUP_SECRETS=()   # VAR=value pairs the user chose to write into the repo .env
SETUP_SELECTED=()  # scratch output of the last _setup_select

# _setup_select "header" "preselected,csv" item...  — leaves the answer in SETUP_SELECTED
# and returns non-zero when the user aborted. A temp file (not a pipeline) so the
# selector's exit status survives.
_setup_select() {
  local header="$1" pre="$2"; shift 2
  local tmp rc=0 line
  tmp=$(mktemp -t ai-toolbox-select)
  ui_choose_multi "$header" "$pre" "$@" >"$tmp" || rc=$?
  SETUP_SELECTED=()
  if [ "$rc" -eq 0 ]; then
    while IFS= read -r line; do
      [ -n "$line" ] && SETUP_SELECTED+=("$line")
    done <"$tmp"
  fi
  rm -f "$tmp"
  return "$rc"
}

_setup_join() { local IFS=" "; printf '%s' "$*"; }
_setup_csv()  { local IFS=","; printf '%s' "$*"; }
_setup_has()  { local n="$1"; shift; local x; for x in "$@"; do [ "$x" = "$n" ] && return 0; done; return 1; }
_setup_has_harness() { _setup_has "$1" ${SETUP_HARNESSES[@]+"${SETUP_HARNESSES[@]}"}; }

# ----------------------------------------------------------------------------
# the walkthrough
# ----------------------------------------------------------------------------
cmd_setup() {
  [ -t 0 ] && [ -t 1 ] || die "setup is interactive — use 'ai-toolbox init --yes' for an unattended install."
  # shellcheck source=bin/lib/ui.sh
  . "$TOOLBOX_ROOT/bin/lib/ui.sh"

  ui_title "ai-toolbox setup" "$REPO"
  ui_has_gum || ui_note "Tip: install charmbracelet/gum (brew install gum) for the full TUI — plain prompts otherwise."
  ui_note "Everything with content lands once, under .agents/ + AGENTS.md + .mcp.json. Each harness gets pointers."

  local total=8
  _setup_ask_harnesses 1 "$total"
  _setup_ask_knowledge 2 "$total"
  _setup_ask_mcp       3 "$total"
  _setup_ask_hooks     4 "$total"
  _setup_ask_skills    5 "$total"
  _setup_ask_rules     6 "$total"
  _setup_ask_secrets   7 "$total"
  _setup_apply         8 "$total"
}

# --- 1. harnesses ------------------------------------------------------------
_setup_ask_harnesses() {
  ui_step "$1" "$2" "Which harnesses should work in this repo?"
  local all="claude codex pi" labels=() preselected=() h label
  for h in $all; do
    label=$(_setup_harness_label "$h")
    labels+=("$label")
    _setup_harness_present "$h" && preselected+=("$label")
  done
  ui_note "Detected on this machine are pre-checked. The same skills/hooks/MCPs are written in each harness's own format."

  _setup_select "harnesses" "$(_setup_csv ${preselected[@]+"${preselected[@]}"})" "${labels[@]}" \
    || die "cancelled."
  SETUP_HARNESSES=()
  local c
  for c in ${SETUP_SELECTED[@]+"${SETUP_SELECTED[@]}"}; do SETUP_HARNESSES+=("${c%% *}"); done
  [ ${#SETUP_HARNESSES[@]} -gt 0 ] || die "no harness selected — nothing to set up."
  ui_done "harnesses: $(_setup_join "${SETUP_HARNESSES[@]}")"

  _setup_ask_pi_client
  _setup_ask_charter
  _setup_ask_migrate
}

# A repo set up by an older version keeps a copy of every script under each harness dir.
_setup_ask_migrate() {
  local d legacy=""
  for d in .claude/hooks .claude/mcp .codex/hooks .codex/mcp .pi/mcp; do
    [ -d "$REPO/$d" ] && legacy="$legacy$d "
  done
  [ -d "$REPO/.claude/skills" ] && [ ! -L "$REPO/.claude/skills" ] && legacy="$legacy.claude/skills "
  [ -n "$legacy" ] || return 0
  ui_note "This repo still has per-harness copies: $legacy"
  ui_confirm "Fold them into .agents/ first (one copy, configs re-pointed)?" && SETUP_MIGRATE=1
  return 0
}

# Pi's core ships no MCP client; it comes from a package installed once per machine.
_setup_ask_pi_client() {
  _setup_has_harness pi || return 0
  if ! have pi; then
    warn "pi is not on PATH — install it with: npm install -g --ignore-scripts @earendil-works/pi-coding-agent"
  elif _pi_mcp_ready; then
    ui_note "Pi MCP client: $PI_MCP_PKG already installed globally."
  elif ui_confirm "Pi has no built-in MCP client. Install $PI_MCP_PKG globally now?"; then
    SETUP_PI_INIT=1
  else
    warn "Skipping — .pi/mcp.json will be written but pi won't read it until you run 'ai-toolbox pi-init'."
  fi
  return 0
}

# The base charter is the one always-on artifact, and it is per machine, not per repo.
_setup_ask_charter() {
  local missing=() h f
  for h in "${SETUP_HARNESSES[@]}"; do
    f=$(_setup_charter_path "$h")
    grep -qF '<!-- ai-toolbox:base-charter -->' "$f" 2>/dev/null || missing+=("${f/#$HOME/~}")
  done
  [ ${#missing[@]} -gt 0 ] || return 0
  ui_note "Base charter (universal engineering norms) is missing from: $(_setup_join "${missing[@]}")"
  ui_confirm "Append the base charter to those global config files?" && SETUP_CHARTER=1
  return 0
}

# --- 2. knowledge files ------------------------------------------------------
_setup_ask_knowledge() {
  ui_step "$1" "$2" "Project knowledge files"
  if [ -f "$REPO/AGENTS.md" ]; then
    ui_done "AGENTS.md already exists — left alone."
    return 0
  fi
  local also=""
  _setup_has_harness claude && also=" + CLAUDE.md (a thin @AGENTS.md adapter)"
  ui_note "AGENTS.md is the cross-harness source of truth — Codex and Pi read it natively, Claude Code imports it."
  ui_confirm "Scaffold AGENTS.md$also?" && SETUP_SCAFFOLD=1
  return 0
}

# --- 3. MCP servers ----------------------------------------------------------
_setup_ask_mcp() {
  ui_step "$1" "$2" "MCP servers"
  compute_reco
  ui_note "Detected stack: $(_setup_join "${DETECTED[@]}")"
  local presets=() f
  for f in "$TOOLBOX_ROOT"/mcp/presets/*.json; do presets+=("$(basename "$f" .json)"); done
  _setup_select "mcp presets (recommended are pre-checked)" \
    "$(_setup_csv ${REC_MCP[@]+"${REC_MCP[@]}"})" "${presets[@]}" || die "cancelled."
  SETUP_MCP=(${SETUP_SELECTED[@]+"${SETUP_SELECTED[@]}"})
  ui_done "mcp: ${SETUP_MCP[*]:-(none)}"
}

# --- 4. hooks ----------------------------------------------------------------
_setup_ask_hooks() {
  ui_step "$1" "$2" "Hooks"
  if ! _setup_has_harness claude && ! _setup_has_harness codex; then
    ui_note "Only pi selected — pi has no shell-hook system (its extensions are TypeScript), so hooks are skipped."
    return 0
  fi
  local hooks=() f b
  for f in "$TOOLBOX_ROOT"/hooks/*.sh; do b=$(basename "$f" .sh); [ "$b" = "_lib" ] || hooks+=("$b"); done
  _setup_select "hooks (recommended are pre-checked)" \
    "$(_setup_csv ${REC_HOOKS[@]+"${REC_HOOKS[@]}"})" "${hooks[@]}" || die "cancelled."
  SETUP_HOOKS=(${SETUP_SELECTED[@]+"${SETUP_SELECTED[@]}"})
  ui_done "hooks: ${SETUP_HOOKS[*]:-(none)}"
}

# --- 5. skills ---------------------------------------------------------------
_setup_ask_skills() {
  ui_step "$1" "$2" "Skills"
  local skills=() d c b
  for d in "$TOOLBOX_ROOT"/skills/*/; do
    b=$(basename "$d")
    if [ -f "$d/SKILL.md" ]; then skills+=("$b")
    else for c in "$d"*/; do [ -f "$c/SKILL.md" ] && skills+=("$b/$(basename "$c")"); done; fi
  done
  _setup_select "skills (recommended are pre-checked)" \
    "$(_setup_csv ${REC_SKILLS[@]+"${REC_SKILLS[@]}"})" "${skills[@]}" || die "cancelled."
  SETUP_SKILLS=(${SETUP_SELECTED[@]+"${SETUP_SELECTED[@]}"})
  ui_done "skills: ${SETUP_SKILLS[*]:-(none)}"
}

# --- 6. rule snippets --------------------------------------------------------
_setup_ask_rules() {
  ui_step "$1" "$2" "Stack rule snippets"
  if [ ${#REC_RULES[@]} -eq 0 ]; then
    ui_note "No starter snippets match this stack."
    return 0
  fi
  local rules=() f b
  for f in "$TOOLBOX_ROOT"/starters/rules/*.md; do b=$(basename "$f" .md); [ "$b" = "README" ] || rules+=("$b"); done
  _setup_select "rule snippets to inline into AGENTS.md" \
    "$(_setup_csv "${REC_RULES[@]}")" "${rules[@]}" || die "cancelled."
  SETUP_RULES=(${SETUP_SELECTED[@]+"${SETUP_SELECTED[@]}"})
  [ ${#SETUP_RULES[@]} -gt 0 ] || return 0
  ui_note "Snippets are seeds, not gospel — whatever lands in AGENTS.md still needs trimming to this repo."
  local def=no
  [ "$SETUP_SCAFFOLD" -eq 1 ] && def=yes
  ui_confirm "Append them to AGENTS.md (marked, for you to trim)?" "$def" && SETUP_APPEND_RULES=1
  ui_done "rules: ${SETUP_RULES[*]}"
}

# --- 7. MCP secrets ----------------------------------------------------------
# Every secret-bearing preset reads its token from the repo .env at launch (with-dotenv.sh
# for stdio, dotenv-header.sh for HTTP headers), so a missing var is the single most common
# reason a freshly configured server won't connect. Close that gap here.
_setup_ask_secrets() {
  ui_step "$1" "$2" "MCP secrets"
  if [ ${#SETUP_MCP[@]} -eq 0 ]; then
    ui_note "No MCP servers selected."
    return 0
  fi
  local paths=() n
  for n in "${SETUP_MCP[@]}"; do paths+=("$TOOLBOX_ROOT/mcp/presets/$n.json"); done

  local vars
  vars=$( { grep -hoE '\$\{[A-Z_][A-Z0-9_]*\}' "${paths[@]}"
            grep -hoE '"--need",[[:space:]]*"[A-Z_][A-Z0-9_]*"' "${paths[@]}"
            grep -hoE 'dotenv-header\.sh[[:space:]]+[A-Z_][A-Z0-9_]*' "${paths[@]}"; } 2>/dev/null \
          | grep -oE '[A-Z_][A-Z0-9_]*' \
          | grep -vE '^(HOME|USER|LOGNAME|PATH|PWD|SHELL|TMPDIR|LANG|TERM)$' | sort -u || true)
  if [ -z "$vars" ]; then
    ui_note "None of the selected servers need a secret."
    return 0
  fi

  local env_file="$REPO/.env" missing=() v
  for v in $vars; do
    if grep -qE "^[[:space:]]*(export[[:space:]]+)?$v=" "$env_file" 2>/dev/null; then
      ui_done "$v found in .env"
    elif [ -n "${!v:-}" ]; then
      ui_done "$v exported in the environment"
    else
      missing+=("$v")
    fi
  done
  [ ${#missing[@]} -gt 0 ] || return 0

  warn "Missing: $(_setup_join "${missing[@]}")"
  ui_note "These are read from $env_file at connect time — no shell export needed."
  ui_confirm "Enter the missing values now (written to .env, never to the committed config)?" no || return 0

  # git itself is the authority here: a global excludes file or a parent .gitignore
  # counts just as much as the repo's own, and a false alarm teaches people to ignore us.
  if git -C "$REPO" rev-parse --git-dir >/dev/null 2>&1 \
     && ! git -C "$REPO" check-ignore -q .env 2>/dev/null; then
    warn ".env is not git-ignored here — add it to .gitignore before you commit."
  fi
  local val
  for v in "${missing[@]}"; do
    val=$(ui_secret "$v")
    [ -n "$val" ] && SETUP_SECRETS+=("$v=$val")
  done
  return 0
}

# --- 8. plan, apply, verify --------------------------------------------------
_setup_apply() {
  ui_step "$1" "$2" "Review"
  local lines=() also=""
  _setup_has_harness claude && also=" + CLAUDE.md"
  lines+=("repo        $REPO")
  lines+=("harnesses   $(_setup_join "${SETUP_HARNESSES[@]}")")
  [ "$SETUP_MIGRATE" -eq 1 ] && lines+=("migrate     fold per-harness copies into .agents/")
  [ "$SETUP_PI_INIT" -eq 1 ]  && lines+=("pi client   install $PI_MCP_PKG globally")
  [ "$SETUP_CHARTER" -eq 1 ]  && lines+=("charter     append to each harness's global config")
  [ "$SETUP_SCAFFOLD" -eq 1 ] && lines+=("knowledge   scaffold AGENTS.md$also")
  [ ${#SETUP_MCP[@]}    -gt 0 ] && lines+=("mcp         ${SETUP_MCP[*]}")
  [ ${#SETUP_HOOKS[@]}  -gt 0 ] && lines+=("hooks       ${SETUP_HOOKS[*]}")
  [ ${#SETUP_SKILLS[@]} -gt 0 ] && lines+=("skills      ${SETUP_SKILLS[*]}")
  [ "$SETUP_APPEND_RULES" -eq 1 ] && lines+=("rules       append ${SETUP_RULES[*]} to AGENTS.md")
  [ ${#SETUP_SECRETS[@]} -gt 0 ] && lines+=("secrets     write ${#SETUP_SECRETS[@]} value(s) to .env")
  printf '\n'; printf '  %s\n' "${lines[@]}"; printf '\n'
  ui_confirm "Apply this plan?" || { info "Nothing written."; return 0; }

  # From here on every write goes through the same subcommands the CLI exposes.
  HARNESS=$(_setup_csv "${SETUP_HARNESSES[@]}")
  ui_step "$1" "$2" "Installing"

  # pi install can prompt, so it runs unspun; the subshell keeps its die() from taking
  # the walkthrough down with it — a missing MCP client is a warning, not a dead end.
  [ "$SETUP_MIGRATE" -eq 1 ] && cmd_migrate
  if [ "$SETUP_PI_INIT" -eq 1 ]; then
    ( cmd_pi_init ) || warn "pi-init failed — run 'ai-toolbox pi-init' by hand."
  fi
  [ "$SETUP_CHARTER" -eq 1 ] && cmd_base_charter
  [ "$SETUP_SCAFFOLD" -eq 1 ] && _setup_scaffold_knowledge
  [ ${#SETUP_HOOKS[@]}  -gt 0 ] && cmd_hooks "${SETUP_HOOKS[@]}"
  [ ${#SETUP_MCP[@]}    -gt 0 ] && cmd_mcp "${SETUP_MCP[@]}"
  [ ${#SETUP_SKILLS[@]} -gt 0 ] && cmd_skill "${SETUP_SKILLS[@]}"
  [ "$SETUP_APPEND_RULES" -eq 1 ] && _setup_append_rules
  [ ${#SETUP_SECRETS[@]} -gt 0 ] && _setup_write_secrets

  _setup_next_steps
}

_setup_scaffold_knowledge() {
  if [ -f "$REPO/AGENTS.md" ]; then info "AGENTS.md already exists — leaving it."
  else cp "$TOOLBOX_ROOT/templates/AGENTS.template.md" "$REPO/AGENTS.md"; ok "scaffolded AGENTS.md"; fi
  _setup_has_harness claude || return 0
  if [ -f "$REPO/CLAUDE.md" ]; then info "CLAUDE.md already exists — leaving it."
  else cp "$TOOLBOX_ROOT/templates/CLAUDE.template.md" "$REPO/CLAUDE.md"; ok "scaffolded CLAUDE.md (@AGENTS.md adapter)"; fi
}

_setup_append_rules() {
  local marker="<!-- ai-toolbox:starter-rules -->"
  if grep -qF "$marker" "$REPO/AGENTS.md" 2>/dev/null; then
    info "starter rules already appended to AGENTS.md — leaving it."; return 0
  fi
  { printf '\n%s\n' "$marker"
    printf '<!-- Seeds from ai-toolbox starters — trim to what is actually true here, then delete this marker. -->\n'
    cmd_rules "${SETUP_RULES[@]}" 2>/dev/null
  } >> "$REPO/AGENTS.md"
  ok "appended ${#SETUP_RULES[@]} rule snippet(s) to AGENTS.md (trim them)"
}

_setup_write_secrets() {
  local pair
  for pair in "${SETUP_SECRETS[@]}"; do
    printf '%s\n' "$pair" >> "$REPO/.env"
    ok "wrote ${pair%%=*} to .env"
  done
}

_setup_next_steps() {
  ui_title "Done — how to start"
  local h
  for h in "${SETUP_HARNESSES[@]}"; do
    case "$h" in
      claude)
        printf '\n  %sclaude%s\n' "$_bld" "$_rst"
        printf '    run `claude` here, then `/mcp` to connect (and finish any OAuth) and `/hooks` to confirm the hooks.\n' ;;
      codex)
        printf '\n  %scodex%s\n' "$_bld" "$_rst"
        printf '    run `codex` here and trust the project — Codex ignores .codex/ entirely until you do.\n'
        printf '    Then `codex mcp login <server>` for OAuth servers.\n' ;;
      pi)
        printf '\n  %spi%s\n' "$_bld" "$_rst"
        printf '    run `pi` here and answer "Trust project folder?" with trust — that is what lets pi load .agents/skills.\n'
        printf '    Then `/mcp` for server status; OAuth servers open a browser on first use.\n'
        _pi_mcp_ready || printf '    %s! MCP is not enabled yet — run `ai-toolbox pi-init` first.%s\n' "$_ylw" "$_rst" ;;
    esac
  done
  printf '\n'
  ui_note "'ai-toolbox status' shows what is installed; re-run 'ai-toolbox setup' any time — it is idempotent."
}

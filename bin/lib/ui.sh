#!/usr/bin/env bash
# ui.sh — prompt/render primitives for `ai-toolbox setup`, sourced by bin/ai-toolbox.
#
# Two backends behind one API. When charmbracelet/gum is on PATH we get real TUI widgets
# (filterable multi-select, masked input, bordered panels); without it every
# function falls back to a plain read-based prompt so the walkthrough still runs on a
# bare machine, over SSH, or in CI. Nothing else in the toolkit depends on gum — it is
# a nicety, never a requirement, which is why the fallbacks are first-class and not
# stubs. Callers must not branch on the backend; ask ui_* and read the answer.
#
# Every selector prints its answer on stdout (one item per line) and returns non-zero
# only when the user aborted (ESC / Ctrl-C), which the walkthrough treats as "quit".
#
# shellcheck shell=bash
# shellcheck disable=SC2154  # _bld/_dim/_rst/_ylw are the caller's output helpers

# AI_TOOLBOX_NO_GUM=1 forces the plain backend (useful for scripted runs and dumb terminals).
UI_GUM=0
[ -z "${AI_TOOLBOX_NO_GUM:-}" ] && command -v gum >/dev/null 2>&1 && UI_GUM=1

# gum's 256-color palette, kept in one place so the whole walkthrough looks the same.
UI_C_ACCENT=212   # pink   — titles, selection cursor
UI_C_MUTED=244    # grey   — help text
UI_C_OK=42        # green  — done markers

ui_has_gum() { [ "$UI_GUM" -eq 1 ]; }

# A bordered title panel — the only "big" chrome in the walkthrough. Returns 0 even when
# there is no subtitle: callers run under `set -e`, so a bare `ui_title "x"` must not fail.
ui_title() {
  if ui_has_gum; then
    gum style --border rounded --border-foreground "$UI_C_ACCENT" --padding "0 2" --margin "1 0 0 0" "$@"
  else
    printf '\n%s%s%s\n' "$_bld" "$1" "$_rst"
    [ $# -gt 1 ] && printf '%s%s%s\n' "$_dim" "$2" "$_rst"
  fi
  return 0
}

# Numbered section header: `ui_step 3 8 "Pick MCP servers"`.
ui_step() {
  local n="$1" total="$2" title="$3"
  if ui_has_gum; then
    printf '\n%s %s\n' \
      "$(gum style --foreground "$UI_C_MUTED" "[$n/$total]")" \
      "$(gum style --bold --foreground "$UI_C_ACCENT" "$title")"
  else
    printf '\n%s[%s/%s] %s%s\n' "$_bld" "$n" "$total" "$title" "$_rst"
  fi
}

ui_note() {
  if ui_has_gum; then gum style --foreground "$UI_C_MUTED" "$@"
  else printf '%s%s%s\n' "$_dim" "$*" "$_rst"; fi
}

ui_done() {
  if ui_has_gum; then printf '%s %s\n' "$(gum style --foreground "$UI_C_OK" "✓")" "$*"
  else ok "$*"; fi
}

# ui_confirm "question" [yes|no]   — default answer used for a bare Enter.
ui_confirm() {
  local q="$1" def="${2:-yes}" reply
  if ui_has_gum; then
    if [ "$def" = "no" ]; then gum confirm --default=false "$q"; else gum confirm "$q"; fi
    return $?
  fi
  local hint="[Y/n]"; [ "$def" = "no" ] && hint="[y/N]"
  printf '%s %s ' "$q" "$hint" >&2
  read -r reply </dev/tty || return 1
  case "${reply:-$def}" in [yY]*|yes) return 0 ;; *) return 1 ;; esac
}

# Show the whole list when it fits rather than gum's default 10-line window, so a
# 13-preset menu doesn't arrive pre-scrolled. gum clamps to the terminal itself.
_ui_height() {
  local n=$(($1 + 2))
  [ "$n" -gt 20 ] && n=20
  printf '%s' "$n"
}

# ui_choose_multi "header" "preselected,csv" item...   — prints chosen items, one per line.
# A preselected item is checked on entry; an empty selection is a legitimate answer.
ui_choose_multi() {
  local header="$1" preselected="$2"; shift 2
  [ $# -gt 0 ] || return 0
  if ui_has_gum; then
    gum choose --no-limit --header "$header" --height "$(_ui_height $#)" \
      --cursor.foreground "$UI_C_ACCENT" --header.foreground "$UI_C_MUTED" \
      ${preselected:+--selected "$preselected"} "$@"
    return $?
  fi
  _ui_choose_multi_fallback "$header" "$preselected" "$@"
}

# Numbered checklist: Enter accepts the defaults, "3 5" toggles those entries,
# "none" clears, "all" selects everything. Re-renders after each toggle.
_ui_choose_multi_fallback() {
  local header="$1" preselected="$2"; shift 2
  local items=("$@") checked=() i reply tok
  for i in "${!items[@]}"; do
    case ",$preselected," in *",${items[$i]},"*) checked[$i]=1 ;; *) checked[$i]=0 ;; esac
  done
  while :; do
    printf '%s%s%s\n' "$_dim" "$header" "$_rst" >&2
    for i in "${!items[@]}"; do
      printf '  %2d) [%s] %s\n' "$((i + 1))" "$([ "${checked[$i]}" -eq 1 ] && echo x || echo ' ')" "${items[$i]}" >&2
    done
    printf '%stoggle by number (space-separated), "all", "none", or Enter to accept: %s' "$_dim" "$_rst" >&2
    read -r reply </dev/tty || return 1
    [ -z "$reply" ] && break
    case "$reply" in
      all)  for i in "${!items[@]}"; do checked[$i]=1; done ;;
      none) for i in "${!items[@]}"; do checked[$i]=0; done ;;
      *)
        for tok in $reply; do
          case "$tok" in
            [0-9]*) i=$((tok - 1))
              [ "$i" -ge 0 ] && [ "$i" -lt "${#items[@]}" ] \
                && checked[$i]=$((1 - checked[$i])) ;;
          esac
        done ;;
    esac
  done
  for i in "${!items[@]}"; do [ "${checked[$i]}" -eq 1 ] && printf '%s\n' "${items[$i]}"; done
  return 0
}

# ui_secret "VAR"   — read one value without echoing it; prints it on stdout.
ui_secret() {
  local var="$1" val
  if ui_has_gum; then
    gum input --password --prompt "$var = " --placeholder "paste the value, or leave empty to skip"
    return $?
  fi
  printf '%s = ' "$var" >&2
  read -rs val </dev/tty || return 1
  printf '\n' >&2
  printf '%s' "$val"
}


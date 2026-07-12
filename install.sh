#!/usr/bin/env bash
# install.sh — make the `ai-toolbox` command available everywhere. Run once,
# after cloning:   cd ai-toolbox && ./install.sh
#
# It symlinks `ai-toolbox` into a bin dir that's already on your PATH (preferring
# ~/.local/bin) so you never edit PATH by hand. If no such dir exists, it creates
# ~/.local/bin and adds it to your shell rc (marker-guarded, idempotent). The link
# points at bin/ai-toolbox in this clone, so `git pull` updates the command with
# no re-install. Re-running is safe. Uninstall: delete the symlink (path printed
# below) and remove the marked block from your shell rc if one was added.
set -euo pipefail

ROOT=$(cd -P "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BIN="$ROOT/bin/ai-toolbox"
[ -f "$BIN" ] || { echo "install: $BIN not found — run this from the ai-toolbox clone." >&2; exit 1; }

# make every shipped script executable (git may not preserve the bit on all clones)
chmod +x "$BIN" 2>/dev/null || true
chmod +x "$ROOT"/hooks/*.sh "$ROOT"/mcp/*.sh 2>/dev/null || true

# pick a target bin dir already on PATH; else create ~/.local/bin and wire PATH.
target=""
for d in "$HOME/.local/bin" "$HOME/bin"; do
  case ":$PATH:" in *":$d:"*) target="$d"; break ;; esac
done

rc=""; added_path=0
if [ -z "$target" ]; then
  target="$HOME/.local/bin"
  case "${SHELL:-}" in
    */zsh) rc="$HOME/.zshrc" ;;
    */bash) rc="$HOME/.bashrc" ;;
    *) rc="$HOME/.profile" ;;
  esac
  local_marker="# ai-toolbox PATH"
  if ! grep -qF "$local_marker" "$rc" 2>/dev/null; then
    printf '\n%s\nexport PATH="%s:$PATH"\n' "$local_marker" "$target" >> "$rc"
    added_path=1
  fi
fi

mkdir -p "$target"
ln -sf "$BIN" "$target/ai-toolbox"

printf '\033[32m✓\033[0m linked ai-toolbox -> %s/ai-toolbox\n' "$target"
if [ "$added_path" -eq 1 ]; then
  printf '\033[32m✓\033[0m added %s to PATH in %s — run "source %s" or open a new shell.\n' "$target" "$rc" "$rc"
fi

cat <<'EOF'

Done. Next:
  ai-toolbox base-charter          # once per machine: always-on charter -> ~/.claude/CLAUDE.md and/or ~/.codex/AGENTS.md
  cd <your repo> && ai-toolbox init   # scaffold AGENTS.md + install the tailored tooling (auto-detects Claude Code / Codex)

Run `ai-toolbox help` for everything.
EOF

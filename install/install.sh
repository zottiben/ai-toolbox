#!/usr/bin/env sh
# Install ai-toolbox: the `ai-toolbox` binary, the board as a desktop app on macOS, and
# the catalogue it reads.
#
#   curl -fsSL https://zottiben.github.io/ai-toolbox/install.sh | sh
#
# Then:  ai-toolbox ui          # the board
#        ai-toolbox init        # set up the repo you are in
set -eu

REPO="zottiben/ai-toolbox"
REPO_URL="https://github.com/${REPO}"
# The catalogue lives here for a downloaded binary. It is a git clone on purpose: the
# hooks, presets and skills are read from disk at run time, so `git pull` updates what
# the tool offers and your own additions survive.
CLONE="${HOME}/.ai-toolbox/clone"

say()  { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
ok()   { printf '\033[32m✓\033[0m %s\n' "$1"; }
warn() { printf '\033[33m!\033[0m %s\n' "$1" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# A script in a clone can use its sibling files. A script piped to `sh` cannot: there
# `$0` is just "sh", and treating the current directory as its source tree can make an
# unrelated Cargo.toml win by accident.
here=""
# shellcheck disable=SC1007 # CDPATH is intentionally empty for this one command.
case "$0" in
  */*) here=$(CDPATH= cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd || true) ;;
esac

from_source=no
for arg in "$@"; do
  case "$arg" in
    --from-source) from_source=yes ;;
    -h|--help)
      echo "usage: install.sh [--from-source]"
      echo "  --from-source  build with cargo instead of downloading a release"
      exit 0 ;;
    *) die "unknown argument: $arg" ;;
  esac
done

command -v git >/dev/null 2>&1 \
  || die "git is required - the catalogue is a clone so that updates and your own additions both work"

# Pick a binary directory already on PATH, without sudo when possible.
if echo "$PATH" | tr ':' '\n' | grep -qx "$HOME/.local/bin"; then
  BIN_DIR="$HOME/.local/bin"
elif echo "$PATH" | tr ':' '\n' | grep -qx "$HOME/.cargo/bin"; then
  BIN_DIR="$HOME/.cargo/bin"
else
  BIN_DIR="/usr/local/bin"
fi

# --- the catalogue ---------------------------------------------------------------
#
# The binary is versioned and replaceable; this is content and stays live. Updating in
# place rather than reclosing keeps anything you have added to it.
install_catalogue() {
  if [ -d "$CLONE/.git" ]; then
    say "Updating the catalogue in $CLONE"
    if git -C "$CLONE" pull --ff-only --quiet 2>/dev/null; then
      ok "catalogue updated"
    else
      # A local commit or a dirty tree is somebody's own work, not a problem to solve
      # by force.
      warn "could not fast-forward $CLONE - leaving it as it is"
    fi
    return 0
  fi
  if [ -e "$CLONE" ]; then
    die "$CLONE exists and is not a git clone - move it aside and re-run"
  fi
  say "Cloning the catalogue to $CLONE"
  mkdir -p "$(dirname "$CLONE")"
  git clone --depth 1 --quiet "$REPO_URL" "$CLONE" \
    || die "could not clone $REPO_URL"
  ok "catalogue cloned"
}

# --- prebuilt release -------------------------------------------------------------
#
# Preferred, because it needs no Rust toolchain and takes seconds. The board is compiled
# into the binary either way, so a downloaded ai-toolbox has the full UI.
install_release() {
  command -v curl >/dev/null 2>&1 || return 1
  command -v tar >/dev/null 2>&1 || return 1

  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  case "$os" in darwin|linux) ;; *) return 1 ;; esac

  version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
    | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
  [ -n "$version" ] || return 1
  num="${version#v}"
  base="${REPO_URL}/releases/download/${version}"

  if [ "$os" = "darwin" ]; then
    file="ai-toolbox-v${num}-macos-universal.tar.gz"
  else
    case "$arch" in
      x86_64|amd64)  larch=x86_64 ;;
      arm64|aarch64) larch=aarch64 ;;
      *) return 1 ;;
    esac
    file="ai-toolbox-v${num}-linux-${larch}.tar.gz"
  fi

  tmp=$(mktemp -d) || return 1
  trap 'rm -rf "$tmp"' EXIT HUP INT TERM

  say "Downloading ai-toolbox ${version}"
  curl -fsSL "${base}/${file}" -o "${tmp}/${file}" || return 1

  # Best effort: only when checksums are published and a hasher exists.
  if curl -fsSL "${base}/checksums.txt" -o "${tmp}/checksums.txt" 2>/dev/null; then
    expected=$(grep " ${file}\$" "${tmp}/checksums.txt" | awk '{print $1}')
    if [ -n "$expected" ]; then
      if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "${tmp}/${file}" | awk '{print $1}')
      elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "${tmp}/${file}" | awk '{print $1}')
      else
        actual=""
      fi
      [ -z "$actual" ] || [ "$actual" = "$expected" ] \
        || die "checksum mismatch for ${file}"
    fi
  fi

  tar xzf "${tmp}/${file}" -C "$tmp" || return 1

  mkdir -p "$BIN_DIR" 2>/dev/null || true
  if [ -w "$BIN_DIR" ]; then
    install -m 0755 "${tmp}/ai-toolbox" "${BIN_DIR}/ai-toolbox"
  else
    sudo install -m 0755 "${tmp}/ai-toolbox" "${BIN_DIR}/ai-toolbox"
  fi
  ok "ai-toolbox installed to ${BIN_DIR}/ai-toolbox"

  # The desktop board, when the archive carries one. `ai-toolbox ui` works regardless;
  # this is for people who would rather have it in the Dock.
  if [ -d "${tmp}/ai-toolbox.app" ]; then
    rm -rf "/Applications/ai-toolbox.app" 2>/dev/null || true
    if cp -R "${tmp}/ai-toolbox.app" /Applications/ 2>/dev/null; then
      ok "ai-toolbox.app installed to /Applications"
    else
      warn "could not write /Applications - run 'ai-toolbox ui' in a browser instead"
    fi
  fi
  return 0
}

install_from_source() {
  command -v cargo >/dev/null 2>&1 \
    || die "no release for this platform and cargo is not installed - get Rust from https://rustup.rs"

  say "Building ai-toolbox"
  if [ -n "$here" ] && [ -f "$here/Cargo.toml" ]; then
    cargo install --path "$here/crates/ai-toolbox" --locked
  else
    # Build from the clone that was just fetched, so the binary and the catalogue are
    # the same revision.
    cargo install --path "$CLONE/crates/ai-toolbox" --locked
  fi
  ok "ai-toolbox installed"
}

install_catalogue

if [ "$from_source" = yes ]; then
  install_from_source
elif install_release; then
  :
else
  warn "no prebuilt release for this platform - building from source"
  install_from_source
fi

TOOLBOX=$(command -v ai-toolbox 2>/dev/null || printf '%s' "${BIN_DIR}/ai-toolbox")

if [ ! -x "$TOOLBOX" ]; then
  die "ai-toolbox is not on PATH - add ${BIN_DIR} to it and re-run"
fi

# Prove the two halves found each other before claiming success. A binary that cannot
# see a catalogue is the one failure mode this install has, and it should surface here
# rather than the first time somebody runs a command.
if ! "$TOOLBOX" list >/dev/null 2>&1; then
  die "ai-toolbox installed but cannot read its catalogue - try: AI_TOOLBOX=$CLONE ai-toolbox list"
fi
ok "catalogue readable: $("$TOOLBOX" list | grep -c '^  ') items"

cat <<EOF

Done. Next:
  ai-toolbox ui                      # the board: every repo on this machine
  cd <your repo> && ai-toolbox init  # detect the stack and set the repo up
  ai-toolbox doctor                  # what is broken here, and --fix to repair it
  ai-toolbox worktrees --sync        # bring every worktree into step

Once per machine:
  ai-toolbox base-charter            # the always-on rules, for each harness
  ai-toolbox pi-init                 # only if you use Pi

The catalogue is a clone at ${CLONE}.
Drop your own hook, skill or MCP preset in there and it appears in \`ai-toolbox list\`.
Re-run this installer to update both halves.
EOF

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

# The board captures this log as text, not a terminal escape stream.
if [ ! -t 1 ]; then
  say()  { printf '==> %s\n' "$1"; }
  ok()   { printf '✓ %s\n' "$1"; }
  warn() { printf '! %s\n' "$1" >&2; }
  die()  { printf 'error: %s\n' "$1" >&2; exit 1; }
fi

# A script in a clone can use its sibling files. A script piped to `sh` cannot: there
# `$0` is just "sh", and treating the current directory as its source tree can make an
# unrelated Cargo.toml win by accident.
here=""
# shellcheck disable=SC1007 # CDPATH is intentionally empty for this one command.
case "$0" in
  */*) here=$(CDPATH= cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd || true) ;;
esac

from_source=no
version=""
bin_override=""
APP_DIR="/Applications"
non_interactive=no
catalogue=yes
while [ "$#" -gt 0 ]; do
  case "$1" in
    --from-source) from_source=yes ;;
    --version|--bin-dir|--app-dir)
      [ "$#" -ge 2 ] && [ -n "$2" ] || die "$1 requires a value"
      case "$1" in
        --version) version="$2" ;;
        --bin-dir) bin_override="$2" ;;
        --app-dir) APP_DIR="$2" ;;
      esac
      shift ;;
    --no-app) APP_DIR="" ;;
    --non-interactive) non_interactive=yes ;;
    --no-catalogue) catalogue=no ;;
    -h|--help)
      echo "usage: install.sh [--from-source] [--version TAG] [--bin-dir PATH]"
      echo "  --from-source      build with cargo instead of downloading a release"
      echo "  --version TAG      install this release; never fall back to a source build"
      echo "  --app-dir PATH     macOS app destination (default /Applications)"
      echo "  --no-app           do not install the desktop app"
      echo "  --no-catalogue     leave the catalogue alone (for a working checkout)"
      echo "  --non-interactive  never prompt for sudo"
      exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

[ -z "$version" ] || [ "$from_source" = no ] || die "--version and --from-source cannot be combined"
case "$version" in *[!v0-9A-Za-z.+-]*) die "invalid release tag: $version" ;; esac
if [ "$catalogue" = yes ]; then
  command -v git >/dev/null 2>&1 \
    || die "git is required - the catalogue is a clone so that updates and your own additions both work"
fi

# Pick a binary directory already on PATH, without sudo when possible.
if [ -n "$bin_override" ]; then
  BIN_DIR="$bin_override"
elif echo "$PATH" | tr ':' '\n' | grep -qx "$HOME/.local/bin"; then
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
    if GIT_TERMINAL_PROMPT=0 git -C "$CLONE" pull --ff-only --quiet 2>/dev/null; then
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
  GIT_TERMINAL_PROMPT=0 git clone --depth 1 --quiet "$REPO_URL" "$CLONE" \
    || die "could not clone $REPO_URL"
  ok "catalogue cloned"
}

# --- prebuilt release -------------------------------------------------------------
#
# Preferred, because it needs no Rust toolchain and takes seconds. The board is compiled
# into the binary either way, so a downloaded ai-toolbox has the full UI.
cleanup_release() {
  rm -rf "$tmp"
  if [ -n "$bin_stage" ]; then
    if [ -w "$BIN_DIR" ]; then
      rm -rf "$bin_stage"
    else
      sudo -n rm -rf "$bin_stage" || warn "could not remove staging directory $bin_stage"
    fi
  fi
  [ -z "$app_stage" ] || rm -rf "$app_stage"
}

install_release() {
  command -v curl >/dev/null 2>&1 || return 1
  command -v tar >/dev/null 2>&1 || return 1

  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)
  case "$os" in darwin|linux) ;; *) return 1 ;; esac

  if [ -z "$version" ]; then
    version=$(curl -fsSL --connect-timeout 10 --max-time 30 "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
      | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
  fi
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
  bin_stage=""
  app_stage=""
  trap cleanup_release EXIT
  trap 'exit 1' HUP INT TERM

  say "Downloading ai-toolbox ${version}"
  curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 10 --max-time 600 "${base}/${file}" -o "${tmp}/${file}" || return 1

  # Never replace an installed executable with an unverified download.
  curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 10 --max-time 30 \
    "${base}/checksums.txt" -o "${tmp}/checksums.txt" || die "could not download release checksums"
  expected=$(awk -v name="$file" '$2 == name {print $1}' "${tmp}/checksums.txt")
  [ -n "$expected" ] || die "no checksum for ${file}"
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "${tmp}/${file}" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "${tmp}/${file}" | awk '{print $1}')
  else
    die "sha256sum or shasum is required to verify the release"
  fi
  [ "$actual" = "$expected" ] || die "checksum mismatch for ${file}"

  tar xzf "${tmp}/${file}" -C "$tmp" || return 1

  # Stage on the destination filesystem, then rename. Copying over a running Linux
  # executable fails with ETXTBSY; truncating one on macOS can invalidate its signature.
  mkdir -p "$BIN_DIR" 2>/dev/null || true
  if [ ! -w "$BIN_DIR" ]; then
    [ "$non_interactive" = no ] || die "$BIN_DIR is not writable - update from a terminal with write permission"
    sudo mkdir -p "$BIN_DIR" || die "cannot create $BIN_DIR"
    bin_stage=$(sudo mktemp -d "$BIN_DIR/.ai-toolbox-update.XXXXXX") || die "cannot stage in $BIN_DIR"
    sudo chown "$(id -u):$(id -g)" "$bin_stage" || die "cannot stage in $BIN_DIR"
  else
    bin_stage=$(mktemp -d "$BIN_DIR/.ai-toolbox-update.XXXXXX") || die "cannot stage in $BIN_DIR"
  fi
  install -m 0755 "${tmp}/ai-toolbox" "$bin_stage/ai-toolbox" || die "could not stage ai-toolbox"
  [ "$("$bin_stage/ai-toolbox" --version)" = "ai-toolbox ${num}" ] || die "downloaded binary has the wrong version"

  [ ! -d "$BIN_DIR/ai-toolbox" ] || die "$BIN_DIR/ai-toolbox is a directory, not an executable"
  if [ "$os" = darwin ] && [ -n "$APP_DIR" ]; then
    [ -x "${tmp}/ai-toolbox.app/Contents/MacOS/ai-toolbox-desktop" ] || die "release is missing the desktop app"
  fi
  # Catalogue errors must surface before replacing a working executable. A dirty or
  # diverged clone is only a warning and is never reset or cleaned.
  [ "$catalogue" = no ] || install_catalogue

  backup=""
  if [ -n "$APP_DIR" ] && [ -d "${tmp}/ai-toolbox.app" ]; then
    mkdir -p "$APP_DIR" || die "cannot create $APP_DIR"
    app_stage=$(mktemp -d "$APP_DIR/.ai-toolbox-update.XXXXXX") || die "$APP_DIR is not writable"
    cp -R "${tmp}/ai-toolbox.app" "$app_stage/ai-toolbox.app" || die "could not stage the desktop app"
    if [ -e "$APP_DIR/ai-toolbox.app" ]; then
      # Keep a recoverable copy outside the cleanup trap until both swaps succeed.
      backup="$APP_DIR/ai-toolbox.app.pre-update"
      [ ! -e "$backup" ] || die "$backup already exists - recover or move it before updating"
      mv "$APP_DIR/ai-toolbox.app" "$backup" || die "could not back up the desktop app"
    fi
    if ! mv "$app_stage/ai-toolbox.app" "$APP_DIR/ai-toolbox.app"; then
      [ -z "$backup" ] || mv "$backup" "$APP_DIR/ai-toolbox.app"
      die "could not replace the desktop app"
    fi
  fi

  if [ -w "$BIN_DIR" ]; then
    mv -f "$bin_stage/ai-toolbox" "$BIN_DIR/ai-toolbox" && swapped=yes || swapped=no
  else
    if sudo chown 0:0 "$bin_stage/ai-toolbox" && sudo mv -f "$bin_stage/ai-toolbox" "$BIN_DIR/ai-toolbox"; then
      swapped=yes
    else
      swapped=no
    fi
  fi
  if [ "$swapped" = no ]; then
    if [ -n "$app_stage" ]; then
      rm -rf "$APP_DIR/ai-toolbox.app"
      [ -z "$backup" ] || mv "$backup" "$APP_DIR/ai-toolbox.app"
    fi
    die "could not replace ai-toolbox; previous installation preserved"
  fi
  [ -z "$backup" ] || rm -rf "$backup"
  ok "ai-toolbox installed to ${BIN_DIR}/ai-toolbox"
  [ -z "$app_stage" ] || ok "ai-toolbox.app installed to $APP_DIR"
  return 0
}

install_from_source() {
  command -v cargo >/dev/null 2>&1 \
    || die "no release for this platform and cargo is not installed - get Rust from https://rustup.rs"

  from_source=yes
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

# A pinned release must not quietly become a build of main on a network error.
pinned=no
[ -z "$version" ] || pinned=yes

if [ "$from_source" = yes ]; then
  [ "$catalogue" = no ] || install_catalogue
  install_from_source
elif install_release; then
  :
else
  [ "$pinned" = no ] || die "could not install release $version; no source fallback was attempted"
  warn "no prebuilt release for this platform - building from source"
  [ "$catalogue" = no ] || install_catalogue
  install_from_source
fi

# Check the binary we actually installed, not an older one earlier on PATH.
TOOLBOX="${BIN_DIR}/ai-toolbox"
if [ "$from_source" = yes ]; then
  TOOLBOX="${CARGO_HOME:-$HOME/.cargo}/bin/ai-toolbox"
fi

if [ ! -x "$TOOLBOX" ]; then
  die "ai-toolbox is not on PATH - add ${BIN_DIR} to it and re-run"
fi

# Prove the two halves found each other before claiming success. A binary that cannot
# see a catalogue is the one failure mode this install has, and it should surface here
# rather than the first time somebody runs a command.
if [ "$catalogue" = yes ] && ! "$TOOLBOX" list >/dev/null 2>&1; then
  die "ai-toolbox installed but cannot read its catalogue - try: AI_TOOLBOX=$CLONE ai-toolbox list"
fi
if [ "$catalogue" = yes ]; then
  ok "catalogue readable: $("$TOOLBOX" list | grep -c '^  ') items"
else
  ok "working catalogue checkout left untouched"
fi

# Self-update callers provide their own restart guidance and render this captured log.
[ "$non_interactive" = no ] || exit 0

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
Run \`ai-toolbox update\` (or use Check for updates in the board) for the next release.
EOF

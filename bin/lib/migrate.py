#!/usr/bin/env python3
"""migrate.py - re-point a repo's harness configs at the canonical .agents/ layout.

Usage:  migrate.py <repo> [--dry-run]
Prints one line per change (or per would-be change) and exits non-zero only on error.
The file moves are done by the CLI; this handles the config rewrites that go with them:

  .claude/settings.json   hook commands  .claude/hooks/X  -> .agents/hooks/X
  .codex/config.toml      hook commands  .codex/hooks/X   -> .agents/hooks/X
  .mcp.json               helper paths   .{claude,codex,pi}/mcp/X -> .agents/mcp/X
  .codex/config.toml      helper paths   (same)
  .pi/mcp.json            fold into .mcp.json: anything Claude Code also wants (or is
                          inert to) moves to the shared file; only a genuinely Pi-only
                          server definition stays behind.

Folding .pi/mcp.json is the one judgement call, and the goal is a *correct* config, not
just a moved one. A Pi entry is safe to share when it does not contradict the shared
entry: `lifecycle` and `directTools` are unknown keys to Claude Code and verified inert
(2.1.220), and a definition identical to the shared one is pure duplication. A `headers`
value starting with `!` is NOT safe - Claude Code would send that literal string - so
those stay behind.

Three fix-ups, because older versions of this toolbox emitted Pi config that the adapter
mishandles:
  - `transport` is not a pi-mcp-adapter key at all (it uses `httpTransport`); drop it.
  - `auth: {"type": "oauth"}` is the wrong shape - the adapter takes the string "oauth",
    and an object makes `supportsOAuth()` return false, silently disabling OAuth. Omitting
    the key entirely means deferred auto-detected OAuth, which is the behaviour we want.
  - a Pi override that changes a server's transport kind (a `command` layered over a
    `url`) leaves the merged entry with both, which the adapter rejects outright. That
    entry is stale; drop it and say which preset to re-run.
"""
import sys, os, json

LEGACY_HELPER_DIRS = (".claude/mcp/", ".codex/mcp/", ".pi/mcp/")
LEGACY_HOOK_DIRS = (".claude/hooks/", ".codex/hooks/")
AGENTS_MCP = ".agents/mcp/"
AGENTS_HOOKS = ".agents/hooks/"
# Pi keys that Claude Code ignores, so they can live in the shared .mcp.json.
SHAREABLE_PI_KEYS = {"lifecycle", "directTools", "idleTimeout", "requestTimeoutMs",
                     "exposeResources", "toolPrefix", "includeTools", "excludeTools",
                     "approveTools", "protocolVersion", "disabled"}

changes = []


def note(msg):
    changes.append(msg)


def canonical(s, olds, new):
    if not isinstance(s, str):
        return s
    for old in olds:
        s = s.replace(old, new)
    return s


def rewrite_paths(value):
    """Rewrite legacy per-harness helper/hook paths anywhere in a JSON-ish structure."""
    if isinstance(value, str):
        v = canonical(value, LEGACY_HELPER_DIRS, AGENTS_MCP)
        return canonical(v, LEGACY_HOOK_DIRS, AGENTS_HOOKS)
    if isinstance(value, list):
        return [rewrite_paths(v) for v in value]
    if isinstance(value, dict):
        return {k: rewrite_paths(v) for k, v in value.items()}
    return value


def migrate_json(path, label, dry):
    if not os.path.exists(path):
        return
    with open(path) as f:
        before = f.read()
    doc = rewrite_paths(json.loads(before))
    after = json.dumps(doc, indent=2) + "\n"
    if after == before:
        return
    note("%s: re-pointed at the .agents/ paths" % label)
    if not dry:
        with open(path, "w") as f:
            f.write(after)


def _kind(entry):
    return "url" if "url" in entry else ("command" if "command" in entry else None)


def _normalise_pi(entry):
    """Strip keys older versions emitted that pi-mcp-adapter does not accept."""
    out = dict(entry)
    out.pop("transport", None)
    if not isinstance(out.get("auth", ""), str):
        out.pop("auth", None)
    return out


def fold_pi(repo, dry):
    """Move everything shareable out of .pi/mcp.json and into .mcp.json."""
    pi_path = os.path.join(repo, ".pi", "mcp.json")
    shared_path = os.path.join(repo, ".mcp.json")
    if not os.path.exists(pi_path):
        return
    with open(pi_path) as f:
        pi_doc = json.load(f)
    pi_servers = rewrite_paths(pi_doc.get("mcpServers", {}))
    shared_doc = {"mcpServers": {}}
    if os.path.exists(shared_path):
        with open(shared_path) as f:
            shared_doc = json.load(f)
        shared_doc.setdefault("mcpServers", {})

    kept, moved, stale = {}, 0, []
    for name, raw in pi_servers.items():
        entry = _normalise_pi(raw)
        target = shared_doc["mcpServers"].get(name)
        if target is None:
            # Pi-only server: it belongs in the shared file, where Claude Code sees it too.
            shared_doc["mcpServers"][name] = entry
            moved += 1
            continue
        if _kind(entry) and _kind(target) and _kind(entry) != _kind(target):
            stale.append(name)
            continue
        # The dotenv wrapper is the canonical form now: it works in every harness, so a Pi
        # entry that only differs by having it upgrades the shared definition instead of
        # living on as a separate one.
        if str(entry.get("command", "")).endswith("with-dotenv.sh") \
                and not str(target.get("command", "")).endswith("with-dotenv.sh"):
            target["command"] = entry.pop("command")
            target["args"] = entry.pop("args", [])
            target.pop("env", None)
            note(".mcp.json: %s now uses the shared .agents/mcp/with-dotenv.sh wrapper" % name)
        stay = {}
        for key, val in entry.items():
            if key in SHAREABLE_PI_KEYS:
                target[key] = val
            elif key in target and target[key] == val:
                pass  # identical to the shared definition - pure duplication, drop it
            else:
                stay[key] = val
        if stay:
            kept[name] = stay
        else:
            moved += 1

    if moved:
        note(".pi/mcp.json: folded %d server(s) into .mcp.json" % moved)
    if kept:
        note(".pi/mcp.json: kept %s (cannot be shared with Claude Code)" % ", ".join(sorted(kept)))
    if stale:
        note(".pi/mcp.json: dropped stale %s - it changed the server's transport kind, which "
             "the adapter rejects. Re-run: ai-toolbox mcp %s"
             % (", ".join(sorted(stale)), " ".join(sorted(stale))))
    if dry:
        return
    with open(shared_path, "w") as f:
        json.dump(shared_doc, f, indent=2)
        f.write("\n")
    if kept:
        with open(pi_path, "w") as f:
            json.dump({"mcpServers": kept}, f, indent=2)
            f.write("\n")
    else:
        os.remove(pi_path)
        pi_dir = os.path.join(repo, ".pi")
        if os.path.isdir(pi_dir) and not os.listdir(pi_dir):
            os.rmdir(pi_dir)


def migrate_codex_toml(repo, dry, lib):
    path = os.path.join(repo, ".codex", "config.toml")
    if not os.path.exists(path):
        return
    sys.path.insert(0, lib)
    import codex_toml as ct
    cfg = ct.load(path)
    before = ct.dumps(cfg)
    cfg = rewrite_paths(cfg)
    after = ct.dumps(cfg)
    if after == before:
        return
    note(".codex/config.toml: re-pointed at the .agents/ paths")
    if not dry:
        with open(path, "w") as f:
            f.write(after)


def main(argv):
    if len(argv) < 2:
        sys.stderr.write("usage: migrate.py <repo> [--dry-run]\n")
        return 2
    repo = argv[1]
    dry = "--dry-run" in argv[2:]
    lib = os.path.dirname(os.path.abspath(__file__))

    migrate_json(os.path.join(repo, ".claude", "settings.json"), ".claude/settings.json", dry)
    migrate_json(os.path.join(repo, ".mcp.json"), ".mcp.json", dry)
    migrate_codex_toml(repo, dry, lib)
    fold_pi(repo, dry)

    for c in changes:
        print(c)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

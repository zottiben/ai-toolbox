#!/usr/bin/env python3
"""mcp_pi.py - layer Pi's MCP settings onto the repo's canonical .mcp.json, spilling to
.pi/mcp.json only where Pi genuinely needs a different server definition.

Usage:  mcp_pi.py <.mcp.json> <.pi/mcp.json> <preset1.json> [preset2.json ...]
Prints one warning line per genuine harness difference to stdout for the CLI to surface.

`.mcp.json` is the cross-harness MCP file: Claude Code reads it natively, and so does
Pi's MCP client (the `pi-mcp-adapter` package - verified: a repo with a .mcp.json and no
.pi/ directory at all connects). Only Codex needs a generated copy, in its own TOML.

So the job here is NOT to write a parallel Pi config. It is to add the two Pi-only knobs
to the shared file - Claude Code ignores unknown server keys, verified against 2.1.220 -
and to fall back to `.pi/mcp.json` (the adapter's highest-precedence layer, merged per
server key by key) only for a server whose Pi definition would be *wrong* for Claude Code.

Verified against pi-mcp-adapter 2.21.2 (types.ts `ServerEntry`, server-manager.ts, utils.ts):
  - stdio when `command` is set, HTTP when `url` is set - a server MUST end up with exactly
    one of command/url/socket, so a layer must never change a server's transport kind.
  - `lifecycle`: "lazy" (default) | "eager" | "keep-alive" | "lazy-keep-alive".
  - `directTools: true` registers each tool individually instead of behind the adapter's
    one-tool proxy. The adapter warns past ~75 direct tools; every one sits in the prompt.
  - a remote server with no credentials needs NO `auth` key: `auth === undefined` means
    deferred auto-detected OAuth, which is what we want. Setting it is noise in a shared file.
  - a `headers` value starting with `!` is run as a command at connect time and its stdout
    becomes the value (`!!` escapes). That is Pi's answer to Claude's `headersHelper` - and
    the one thing that cannot be shared, because Claude Code would send the literal string.

Split:
  shared (-> .mcp.json)     lifecycle, directTools
  override (-> .pi/mcp.json) headers (from a headersHelper), httpTransport, and any
                             command/args that had to be rewritten for Pi
"""
import sys, re, json, os, shlex

VARRE = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
# always-set shell basics - not secrets, but Pi still won't interpolate them in `args`,
# so a ${HOME}-style path must be expanded at launch by the wrapper.
SHELL_BASICS = {"HOME", "USER", "LOGNAME", "PATH", "PWD", "SHELL", "TMPDIR", "LANG", "TERM"}
WRAPPER = ".agents/mcp/with-dotenv.sh"
# stdio servers are local, cheap to spawn and small, so connect them at session start like
# Claude Code does and put their tools straight in the prompt. A remote server may block on
# a network round trip or an OAuth browser flow, and the SaaS ones are the tool-heavy ones
# (measured 2026-08-11: pixellab 72 tools, ClickUp 55), so leave those lazy behind the proxy.
LIFECYCLE_STDIO = "eager"
LIFECYCLE_REMOTE = "lazy"


def _canonical(s):
    """Old presets/configs pointed helpers at a per-harness dir; they now live in one place."""
    if not isinstance(s, str):
        return s
    for old in (".claude/mcp/", ".codex/mcp/", ".pi/mcp/"):
        s = s.replace(old, ".agents/mcp/")
    return s


def _split_env(env):
    passthrough, static = [], {}
    for k, v in (env or {}).items():
        if isinstance(v, str) and VARRE.fullmatch(v.strip()):
            passthrough.append(k)  # value is exactly ${VAR} -> read from .env via the wrapper
        else:
            static[k] = v
    return passthrough, static


def _header_from_helper(hh):
    """Map a Claude `headersHelper` command to a Pi `headers` entry.

    The helper prints a JSON headers object for Claude; `--raw` makes it print just the
    header value, which is what Pi's `!command` secret resolution wants.
    """
    toks = shlex.split(hh)
    script, rest = _canonical(toks[0]), toks[1:]
    name = "Authorization"
    for i, t in enumerate(rest):
        if t == "--header" and i + 1 < len(rest):
            name = rest[i + 1]
    var = next((t for t in rest if re.fullmatch(r"[A-Z_][A-Z0-9_]*", t)), None)
    return name, "!" + shlex.join([script, "--raw", *rest]), var


def convert_server(name, c):
    """Return (shared_keys, override_keys, warnings) for one preset server."""
    shared, override, w = {}, {}, []

    # ── remote (streamable-http / sse) ──────────────────────────────────────────
    if c.get("type") in ("http", "sse") or "url" in c:
        if c.get("type") == "sse":
            override["httpTransport"] = "sse"
        hh = c.get("headersHelper")
        if hh:
            hname, hcmd, var = _header_from_helper(hh)
            override["headers"] = {hname: hcmd}
            w.append(
                "%s: %s stays in the repo .env - Pi reads it at connect time via a '!' command "
                "header in .pi/mcp.json (Claude Code keeps using headersHelper in .mcp.json)."
                % (name, var or "the API token")
            )
        shared["lifecycle"] = LIFECYCLE_REMOTE
        return shared, override, w

    # ── stdio ───────────────────────────────────────────────────────────────────
    cmd = _canonical(c.get("command", ""))
    args = [_canonical(a) for a in c.get("args", [])]
    passthrough, static = _split_env(c.get("env", {}))
    argvars = []
    for a in args:
        for m in VARRE.findall(a):
            if m not in argvars:
                argvars.append(m)

    # A ${VAR} that Pi will not interpolate has to be resolved by the wrapper at launch.
    # The shipped presets already use it, so this only fires for a hand-written server.
    if not cmd.endswith("with-dotenv.sh") and (argvars or passthrough):
        needs = [v for v in argvars if v not in SHELL_BASICS]
        for v in passthrough:
            if v not in needs:
                needs.append(v)
        pre = []
        for v in needs:
            pre += ["--need", v]
        override["command"] = WRAPPER
        override["args"] = pre + ["--", cmd] + args
        if static:
            override["env"] = static
        interp = argvars + [v for v in passthrough if v not in argvars]
        w.append(
            "%s: Pi gets a %s wrap in .pi/mcp.json so ${%s} resolve from the repo .env at "
            "launch (Pi never interpolates command/args). Move the server to that wrapper in "
            ".mcp.json to share one definition." % (name, WRAPPER, "}, ${".join(interp))
        )

    shared["lifecycle"] = LIFECYCLE_STDIO
    shared["directTools"] = True
    return shared, override, w


def convert_presets(preset_paths):
    shared, overrides, warnings = {}, {}, []
    for p in preset_paths:
        for name, c in json.load(open(p)).get("mcpServers", {}).items():
            s, o, ws = convert_server(name, c)
            if s:
                shared[name] = s
            if o:
                overrides[name] = o
            warnings.extend(ws)
    return shared, overrides, warnings


def _load(path, default):
    if os.path.exists(path):
        with open(path) as f:
            return json.load(f)
    return default


def _write(path, doc):
    d = os.path.dirname(path)
    if d:
        os.makedirs(d, exist_ok=True)
    with open(path, "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")


def main(argv):
    if len(argv) < 4:
        sys.stderr.write("usage: mcp_pi.py <.mcp.json> <.pi/mcp.json> <preset.json>...\n")
        return 2
    shared_path, pi_path, presets = argv[1], argv[2], argv[3:]
    shared, overrides, warnings = convert_presets(presets)

    # Pi's knobs go on the servers already written to the shared file.
    doc = _load(shared_path, {"mcpServers": {}})
    doc.setdefault("mcpServers", {})
    for name, keys in shared.items():
        if name in doc["mcpServers"]:
            doc["mcpServers"][name].update(keys)
    _write(shared_path, doc)

    # Only touch .pi/mcp.json when something genuinely cannot be shared - an empty
    # override file is one more thing to keep in sync for no reason.
    if overrides:
        pi_doc = _load(pi_path, {"mcpServers": {}})
        pi_doc.setdefault("mcpServers", {})
        pi_doc["mcpServers"].update(overrides)
        _write(pi_path, pi_doc)
    for w in warnings:
        print(w)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

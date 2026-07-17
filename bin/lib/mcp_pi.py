#!/usr/bin/env python3
"""mcp_pi.py - convert ai-toolbox MCP presets (Claude .mcp.json shape) into the
pi-mcp-extension config shape and merge them into a repo .pi/mcp.json.

Usage:  mcp_pi.py <.pi/mcp.json> <preset1.json> [preset2.json ...]
Merges the converted servers into the target file (created if absent, existing
settings/servers preserved) and prints one warning line per genuine harness
difference to stdout for the CLI to surface.

Pi's MCP client is the `pi-mcp-extension` package (pi install npm:pi-mcp-extension).
Config lives at .pi/mcp.json (project) / ~/.pi/agent/mcp.json (global). Verified
against the extension's config.ts (v1.5.0):
  - explicit `transport`: "stdio" | "streamable-http" | "sse"
  - `lifecycle`: "eager" (start at session start) | "lazy" (manual /mcp:start)
  - NO ${VAR} interpolation (WYSIWYG) - secrets come from exported env or a wrapper
  - OAuth: `auth: {type:"oauth"}` on a remote server → SDK does discovery + dynamic
    client registration + PKCE + token refresh (tokens cached to disk)

Conversion rules:
  stdio, no secrets                       -> command/args/env verbatim (+ transport/lifecycle)
  env {K: "${K}"} (host passthrough)      -> route through .pi/mcp/with-dotenv.sh --need K
  ${VAR} inside args (pi won't interp)    -> route through .pi/mcp/with-dotenv.sh (expands at launch)
  env {K: "literal"}                      -> env: {K: "literal"}
  command already = with-dotenv.sh        -> keep (path rewritten to .pi/mcp/)
  type:"http" + url, no header helper     -> transport streamable-http + auth {type:"oauth"}
  pixellab headersHelper                   -> .pi/mcp/with-dotenv.sh + mcp-remote stdio bridge;
                                             expands token only at launch and adds a bearer header
  other headersHelper                      -> streamable-http (no auth) + a warning: pi has no
                                             generic headersHelper/interpolation to read a token from .env
"""
import sys, re, json, os

VARRE = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
# always-set shell basics - not secrets, but pi still won't interpolate them in a
# config string, so ${HOME}-style paths must be expanded at launch by the wrapper.
SHELL_BASICS = {"HOME", "USER", "LOGNAME", "PATH", "PWD", "SHELL", "TMPDIR", "LANG", "TERM"}
WRAPPER = ".pi/mcp/with-dotenv.sh"
LIFECYCLE = "eager"  # tools ready at session start, matching Claude Code's auto-connect


def _rewrite(s):
    return s.replace(".claude/mcp/", ".pi/mcp/") if isinstance(s, str) else s


def _split_env(env):
    passthrough, static = [], {}
    for k, v in (env or {}).items():
        if isinstance(v, str) and VARRE.fullmatch(v.strip()):
            passthrough.append(k)  # value is exactly ${VAR} -> read from .env via wrapper
        else:
            static[k] = v
    return passthrough, static


def convert_server(name, c):
    out, w = {}, []

    # ── remote (streamable-http / sse) ──────────────────────────────────────────
    if c.get("type") in ("http", "sse") or "url" in c:
        out["transport"] = "sse" if c.get("type") == "sse" else "streamable-http"
        out["url"] = c["url"]
        hh = c.get("headersHelper")
        headers = c.get("headers")
        if hh and name == "pixellab":
            # pi-mcp has no dynamic-header hook. mcp-remote is a maintained stdio to
            # streamable-HTTP bridge with --header support; the dotenv wrapper keeps
            # PIXELLAB_API_TOKEN out of config and expands it only at process launch.
            toks = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", hh)
            var = next((t for t in reversed(toks) if t.isupper()), "PIXELLAB_API_TOKEN")
            out = {
                "command": WRAPPER,
                "args": [
                    "--need", var, "--",
                    "npx", "-y", "mcp-remote@latest", c["url"],
                    "--transport", "http-only",
                    "--header", "Authorization:Bearer ${%s}" % var,
                ],
                "transport": "stdio",
                "lifecycle": LIFECYCLE,
            }
            w.append(
                "%s: routed through %s + mcp-remote so %s is read from the repo .env "
                "and sent as a bearer header at launch." % (name, WRAPPER, var)
            )
        elif hh:
            toks = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", hh)
            var = next((t for t in reversed(toks) if t.isupper()), None)
            w.append(
                "%s: pi-mcp has no generic headersHelper and does not interpolate ${VAR}, so %s "
                "can't be read from the repo .env. Add a literal header to .pi/mcp.json by hand "
                "(the toolbox won't write your token)." % (name, var or "the API token")
            )
        elif headers:
            out["headers"] = headers  # static API-key header carried through
        else:
            # remote with no static credential -> OAuth flow (expo, sentry, clickup, figma dev-mode)
            out["auth"] = {"type": "oauth"}
        out["lifecycle"] = LIFECYCLE
        return out, w

    # ── stdio ───────────────────────────────────────────────────────────────────
    cmd = _rewrite(c.get("command", ""))
    args = [_rewrite(a) for a in c.get("args", [])]
    passthrough, static = _split_env(c.get("env", {}))
    argvars = []
    for a in args:
        for m in VARRE.findall(a):
            if m not in argvars:
                argvars.append(m)

    if cmd.endswith("with-dotenv.sh"):
        pre = []
        for v in passthrough:  # ensure host-passthrough secrets load from .env + are required
            pre += ["--need", v]
        out["command"] = cmd
        out["args"] = pre + args
        if static:
            out["env"] = static
        out["transport"] = "stdio"
        out["lifecycle"] = LIFECYCLE
        return out, w

    if argvars or passthrough:
        needs = [v for v in argvars if v not in SHELL_BASICS]
        for v in passthrough:
            if v not in needs:
                needs.append(v)
        pre = []
        for v in needs:
            pre += ["--need", v]
        out["command"] = WRAPPER
        out["args"] = pre + ["--", cmd] + args
        if static:
            out["env"] = static
        out["transport"] = "stdio"
        out["lifecycle"] = LIFECYCLE
        interp = argvars + [v for v in passthrough if v not in argvars]
        w.append(
            "%s: routed through %s so ${%s} resolve from the repo .env at launch "
            "(pi does not interpolate config strings)." % (name, WRAPPER, "}, ${".join(interp))
        )
        return out, w

    out["command"] = cmd
    out["args"] = args
    if static:
        out["env"] = static
    out["transport"] = "stdio"
    out["lifecycle"] = LIFECYCLE
    return out, w


def convert_presets(preset_paths):
    servers, warnings = {}, []
    for p in preset_paths:
        data = json.load(open(p))
        for name, c in data.get("mcpServers", {}).items():
            srv, ws = convert_server(name, c)
            servers[name] = srv
            warnings.extend(ws)
    return servers, warnings


def main(argv):
    if len(argv) < 3:
        sys.stderr.write("usage: mcp_pi.py <.pi/mcp.json> <preset.json>...\n")
        return 2
    tgt_path, presets = argv[1], argv[2:]
    servers, warnings = convert_presets(presets)

    tgt = {"mcpServers": {}}
    if os.path.exists(tgt_path):
        with open(tgt_path) as f:
            tgt = json.load(f)
        tgt.setdefault("mcpServers", {})
    tgt["mcpServers"].update(servers)

    os.makedirs(os.path.dirname(tgt_path), exist_ok=True)
    with open(tgt_path, "w") as f:
        json.dump(tgt, f, indent=2)
        f.write("\n")
    for w in warnings:
        print(w)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

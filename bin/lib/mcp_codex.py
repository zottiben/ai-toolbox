#!/usr/bin/env python3
"""mcp_codex.py — convert ai-toolbox MCP presets (Claude .mcp.json shape) into Codex
config.toml [mcp_servers.*] tables and merge them into a repo/global .codex/config.toml.

Usage:  mcp_codex.py <config.toml> <preset1.json> [preset2.json ...]
Merges the converted servers into <config.toml> (created if absent) and prints one
warning line per genuine harness difference to stdout for the CLI to surface.

Conversion rules (verified against Codex MCP docs + a live config.toml):
  stdio command/args                      -> same keys
  env {K: "${K}"}  (host passthrough)     -> env_vars = ["K"]
  env {K: "literal"}                      -> [mcp_servers.<name>.env] table
  ${VAR} inside args (Codex won't interp) -> route through .codex/mcp/with-dotenv.sh,
                                             which loads .env and expands ${VAR} at launch
  command already = with-dotenv.sh        -> keep (path rewritten to .codex/mcp/)
  type:"http" + url                       -> url = "..."   (OAuth via `codex mcp login`)
  headersHelper (Claude-only)             -> bearer_token_env_var + a warning
"""
import sys, re, json, os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import codex_toml as ct

VARRE = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
# always-set shell basics — not secrets, but Codex still won't interpolate them in a
# config string, so ${HOME}-style paths must be expanded at launch by the wrapper.
SHELL_BASICS = {"HOME", "USER", "LOGNAME", "PATH", "PWD", "SHELL", "TMPDIR", "LANG", "TERM"}
WRAPPER = ".codex/mcp/with-dotenv.sh"


def _rewrite(s):
    return s.replace(".claude/mcp/", ".codex/mcp/") if isinstance(s, str) else s


def _split_env(env):
    passthrough, static = [], {}
    for k, v in (env or {}).items():
        if isinstance(v, str) and VARRE.fullmatch(v.strip()):
            passthrough.append(k)  # value is exactly ${VAR} -> host-env passthrough
        else:
            static[k] = v
    return passthrough, static


def convert_server(name, c):
    warns, out = [], {}
    if c.get("type") == "http" or "url" in c:
        out["url"] = c["url"]
        hh = c.get("headersHelper")
        if hh:
            toks = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", hh)
            var = next((t for t in reversed(toks) if t.isupper()), None)
            if var:
                out["bearer_token_env_var"] = var
                warns.append(
                    "%s: Codex has no headersHelper equivalent for a remote MCP; mapped to "
                    "bearer_token_env_var=%s. Export %s in the environment you launch codex "
                    "from (Claude Code reads it from the repo .env; Codex does not)." % (name, var, var)
                )
        pv, sv = _split_env(c.get("env", {}))
        if pv:
            out["env_vars"] = pv
        if sv:
            out["env"] = sv
        return out, warns

    cmd = _rewrite(c.get("command", ""))
    args = [_rewrite(a) for a in c.get("args", [])]
    pv, sv = _split_env(c.get("env", {}))
    argvars = []
    for a in args:
        for m in VARRE.findall(a):
            if m not in argvars:
                argvars.append(m)

    if cmd.endswith("with-dotenv.sh"):
        out["command"] = cmd
        out["args"] = args
        if sv:
            out["env"] = sv
        if pv:
            out["env_vars"] = pv
        return out, warns

    if argvars:
        needs = [v for v in argvars if v not in SHELL_BASICS]
        for v in pv:  # env-passthrough secrets: validate + load from .env via the wrapper
            if v not in needs:
                needs.append(v)
        pre = []
        for v in needs:
            pre += ["--need", v]
        out["command"] = WRAPPER
        out["args"] = pre + ["--", cmd] + args
        if sv:
            out["env"] = sv
        warns.append(
            "%s: routed through %s so ${%s} expand from the repo .env at launch "
            "(Codex does not interpolate config strings)." % (name, WRAPPER, "}, ${".join(argvars))
        )
        return out, warns

    out["command"] = cmd
    out["args"] = args
    if sv:
        out["env"] = sv
    if pv:
        out["env_vars"] = pv
    return out, warns


def convert_presets(preset_paths):
    servers, warnings = {}, []
    for p in preset_paths:
        data = json.load(open(p))
        for name, c in data.get("mcpServers", {}).items():
            srv, w = convert_server(name, c)
            servers[name] = srv
            warnings.extend(w)
    return servers, warnings


def main(argv):
    if len(argv) < 3:
        sys.stderr.write("usage: mcp_codex.py <config.toml> <preset.json>...\n")
        return 2
    cfg_path, presets = argv[1], argv[2:]
    servers, warnings = convert_presets(presets)
    cfg = ct.load(cfg_path)
    ct.merge_mcp(cfg, servers)
    with open(cfg_path, "w") as f:
        f.write(ct.dumps(cfg))
    for w in warnings:
        print(w)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

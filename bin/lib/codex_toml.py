#!/usr/bin/env python3
"""codex_toml.py — read/merge/emit a Codex ~/.codex or repo .codex config.toml.

The ai-toolbox CLI stores Codex MCP servers and hooks in config.toml. Python's
stdlib can *read* TOML (tomllib, 3.11+) but not write it, so this module carries a
small, faithful TOML serializer plus the two merge operations the CLI needs:

  - merge_mcp(cfg, servers):  cfg["mcp_servers"][name] = server   (overwrite by name)
  - merge_hooks(cfg, hooks):  union per event/matcher, dedup by command

The serializer is deliberately data-faithful (round-trips tomllib output) rather
than format-preserving: it does not keep comments. The CLI only ever writes the
*project* .codex/config.toml with it (never the user's hand-maintained global one),
and most repos have none — so a fresh, well-formed file is the normal outcome.

CLI:  codex_toml.py <config.toml> <patch.json>
  patch.json = {"mcp_servers": {..}}  and/or  {"hooks": {"PreToolUse": [..], ..}}
Reads config.toml (or {} if absent), merges, writes it back.
"""
import sys, re, json

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    tomllib = None

_BARE = re.compile(r"^[A-Za-z0-9_-]+$")


def _key(k):
    k = str(k)
    return k if _BARE.match(k) else _basic_string(k)


def _dotted(path):
    return ".".join(_key(p) for p in path)


def _basic_string(s):
    out = []
    for ch in str(s):
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        elif ord(ch) < 0x20:
            out.append("\\u%04x" % ord(ch))
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def _scalar(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int) or isinstance(v, float):
        return repr(v)
    if isinstance(v, str):
        return _basic_string(v)
    # datetime or anything unexpected -> render its ISO/str form unquoted-safe as string
    return _basic_string(str(v))


def _is_table(v):
    return isinstance(v, dict)


def _is_aot(v):
    return isinstance(v, list) and len(v) > 0 and all(isinstance(x, dict) for x in v)


def _inline(v):
    # value inside an inline array: scalar, inline array, or inline table
    if isinstance(v, dict):
        return "{ " + ", ".join(_key(k) + " = " + _inline(x) for k, x in v.items()) + " }"
    if isinstance(v, list):
        return "[" + ", ".join(_inline(x) for x in v) + "]"
    return _scalar(v)


def _value(v):
    if isinstance(v, list):
        return "[" + ", ".join(_inline(x) for x in v) + "]"
    return _scalar(v)


def _emit(d, path, lines, is_aot_item=False):
    scalars = [(k, v) for k, v in d.items() if not _is_table(v) and not _is_aot(v)]
    subtables = [(k, v) for k, v in d.items() if _is_table(v)]
    aots = [(k, v) for k, v in d.items() if _is_aot(v)]

    # A non-root table needs its own [header] when it carries scalars, or when it is
    # entirely empty (so it exists at all). Tables holding only sub-tables let their
    # children print the full dotted path. AoT items already printed their [[header]].
    if path and not is_aot_item and (scalars or (not subtables and not aots)):
        lines.append("[" + _dotted(path) + "]")
    for k, v in scalars:
        lines.append(_key(k) + " = " + _value(v))
    if scalars and (subtables or aots):
        lines.append("")
    for k, v in subtables:
        _emit(v, path + [k], lines)
        lines.append("")
    for k, v in aots:
        for item in v:
            lines.append("[[" + _dotted(path + [k]) + "]]")
            _emit(item, path + [k], lines, is_aot_item=True)
            lines.append("")


def dumps(d):
    lines = []
    _emit(d, [], lines)
    text = "\n".join(lines)
    text = re.sub(r"\n{3,}", "\n\n", text).strip("\n")
    return text + "\n" if text else ""


def load(path):
    if tomllib is None:
        raise RuntimeError("tomllib requires Python 3.11+")
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except FileNotFoundError:
        return {}


def merge_mcp(cfg, servers):
    dst = cfg.setdefault("mcp_servers", {})
    for name, server in servers.items():
        dst[name] = server  # overwrite by name (mirrors the Claude .mcp.json merge)
    return cfg


def merge_hooks(cfg, hooks):
    dst = cfg.setdefault("hooks", {})
    for event, matchers in hooks.items():
        lst = dst.setdefault(event, [])
        for m in matchers:
            key = m.get("matcher")
            existing = next((x for x in lst if x.get("matcher") == key), None)
            if existing is None:
                existing = {k: v for k, v in m.items() if k != "hooks"}
                existing["hooks"] = []
                lst.append(existing)
            seen = {h.get("command") for h in existing.get("hooks", [])}
            for h in m.get("hooks", []):
                if h.get("command") not in seen:
                    existing["hooks"].append(h)
                    seen.add(h.get("command"))
    return cfg


def main(argv):
    if len(argv) != 3:
        sys.stderr.write("usage: codex_toml.py <config.toml> <patch.json>\n")
        return 2
    cfg_path, patch_path = argv[1], argv[2]
    cfg = load(cfg_path)
    with open(patch_path) as f:
        patch = json.load(f)
    if "mcp_servers" in patch:
        merge_mcp(cfg, patch["mcp_servers"])
    if "hooks" in patch:
        merge_hooks(cfg, patch["hooks"])
    with open(cfg_path, "w") as f:
        f.write(dumps(cfg))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

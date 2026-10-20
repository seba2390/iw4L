#!/usr/bin/env python3
"""The port graph: one SQLite index of what a builtin, ledger id or entity
event is, where IW4L implements it and what is known about it.

    python3 scripts/port_graph.py build [--quiet] [--db PATH]
    python3 scripts/port_graph.py check [--ci] [--db PATH]
    python3 scripts/port_graph.py builtin <name>... [--game t5]
    python3 scripts/port_graph.py addr <hex>...
    python3 scripts/port_graph.py unimplemented [--game t5] [--realm server|client]
    python3 scripts/port_graph.py ledger [id or substring]
    python3 scripts/port_graph.py event [name]
    python3 scripts/port_graph.py sql "SELECT ..."

The tracked half is read from the tree: the generated builtin catalogs
(`crates/game_*/src/*catalog*.rs`), the natives the script host registers
(`crates/sim/src/script/host/`), the fidelity ledgers (`docs/fidelity/`) with
their `unknown!` and error-id references, and every `EntityEventKind` use.
The local half (the executables' builtin tables, decompiled functions and
their call edges, journal sections, script call sites) comes from
`context/re/re_tables.py` when this checkout has one; CI has none and indexes
the tracked half only.

The index lives at `context/re/port_graph.sqlite` (or the temp directory when
there is no `context/re/`). Every query first compares a fingerprint of its
inputs and rebuilds a stale index, so nothing reads an old one. A build writes
a new file and renames it into place; rows are inserted sorted, so the same
inputs give the same tables.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import sqlite3
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA_VERSION = "1"

SCHEMA = """
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE catalog_builtins(
    game TEXT, realm TEXT, kind TEXT, name TEXT, owner TEXT, dev INT,
    file TEXT, line INT);
CREATE TABLE rust_natives(
    game_scope TEXT, realm TEXT, kind TEXT, name TEXT, status TEXT,
    file TEXT, line INT);
CREATE TABLE exe_builtins(
    game TEXT, realm TEXT, kind TEXT, table_label TEXT, name TEXT, addr TEXT,
    dev INT, decomp_path TEXT);
CREATE TABLE decomp_files(addr TEXT, path TEXT);
CREATE TABLE call_edges(which TEXT, caller TEXT, callee TEXT);
CREATE TABLE journal_refs(token TEXT, kind TEXT, file TEXT, line INT, heading TEXT);
CREATE TABLE gsc_calls(name TEXT, realm TEXT, file TEXT, line INT, snippet TEXT);
CREATE TABLE ledger_items(
    game TEXT, id TEXT, severity TEXT, checked INT, file TEXT, line INT,
    summary TEXT);
CREATE TABLE ledger_code_refs(id TEXT, kind TEXT, file TEXT, line INT);
CREATE TABLE entity_events(event_name TEXT, role TEXT, file TEXT, line INT, detail TEXT);
CREATE TABLE entity_event_actions(action TEXT, role TEXT, file TEXT, line INT, detail TEXT);
CREATE INDEX catalog_name ON catalog_builtins(name);
CREATE INDEX natives_name ON rust_natives(name);
CREATE INDEX exe_name ON exe_builtins(name);
CREATE INDEX exe_addr ON exe_builtins(addr);
CREATE INDEX decomp_addr ON decomp_files(addr);
CREATE INDEX edges_caller ON call_edges(caller);
CREATE INDEX edges_callee ON call_edges(callee);
CREATE INDEX journal_token ON journal_refs(token);
CREATE INDEX gsc_name ON gsc_calls(name);
CREATE INDEX ledger_id ON ledger_items(id);
CREATE INDEX ledger_ref_id ON ledger_code_refs(id);
CREATE INDEX events_name ON entity_events(event_name);
CREATE TABLE entity_event_triggers(action TEXT, event_type TEXT, file TEXT, line INT);
CREATE TABLE bevy_observers(event_type TEXT, fn_name TEXT, file TEXT, line INT);
CREATE INDEX actions_name ON entity_event_actions(action);
CREATE INDEX triggers_action ON entity_event_triggers(action);
CREATE INDEX observers_type ON bevy_observers(event_type);
"""

GAMES = ("iw4", "iw5", "t5", "t6", "t7")
HOST = Path("crates/sim/src/script/host")
CLIENT_RS = Path("crates/sim/src/script/client.rs")

# The registry constructors that make each game's native set
# (`crates/sim/src/script/host/registry.rs`), and the scope each serves.
REGISTRY_ROOTS = {
    "engine_services": "shared/server",
    "with_mw2_systems": "iw4/server",
    "with_black_ops": "t5/server",
    "black_ops_client": "t5/client",
    "black_ops_2": "t6/server",
    "black_ops_2_client": "t6/client",
    "black_ops_3": "t7/server",
}


# ---------------------------------------------------------------- locations


def default_db(root: Path) -> Path:
    re_dir = root / "context" / "re"
    if re_dir.is_dir():
        return re_dir / "port_graph.sqlite"
    return Path(tempfile.gettempdir()) / "iw4l_port_graph.sqlite"


def re_tables_dir(root: Path) -> Path | None:
    """`context/re/` holding `re_tables.py`. A `cargo xtask mr` clone has only
    the bases, as symlinks into the root's `context/re/`; follow one there."""
    re_dir = root / "context" / "re"
    if not re_dir.is_dir():
        return None
    if (re_dir / "re_tables.py").is_file():
        return re_dir
    for entry in sorted(re_dir.iterdir()):
        if entry.is_symlink():
            target = entry.resolve().parent
            if (target / "re_tables.py").is_file():
                return target
    return None


def rel(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def rust_files(root: Path, sub: Path | str) -> list[Path]:
    base = root / sub
    if not base.is_dir():
        return []
    return sorted(p for p in base.rglob("*.rs") if "target" not in p.parts)


# ---------------------------------------------------------------- fingerprint


def fingerprint(root: Path) -> str:
    h = hashlib.sha256()
    h.update(SCHEMA_VERSION.encode())
    h.update(Path(__file__).read_bytes())

    def stat(path: Path) -> None:
        try:
            st = path.stat()
        except OSError:
            return
        h.update(f"{rel(root, path)}\0{st.st_mtime_ns}\0{st.st_size}\n".encode())

    for path in rust_files(root, "crates"):
        stat(path)
    for path in sorted((root / "docs" / "fidelity").glob("*.md")):
        stat(path)
    re_dir = re_tables_dir(root)
    if re_dir is not None:
        h.update(f"re:{re_dir}\n".encode())
        st = (re_dir / "re_tables.py").stat()
        h.update(f"re_tables\0{st.st_mtime_ns}\0{st.st_size}\n".encode())
        c_root = root / "context" / "re" / "filled" / "c"
        if c_root.is_dir():
            for which in sorted(c_root.iterdir()):
                if which.is_dir():
                    count = sum(1 for _ in which.glob("*.c"))
                    h.update(
                        f"c:{which.name}:{count}:{which.stat().st_mtime_ns}\n".encode()
                    )
        art = root / "context" / "artifacts"
        if art.is_dir():
            for md in sorted(art.rglob("*.md")):
                st = md.stat()
                h.update(
                    f"a:{md.relative_to(art)}:{st.st_mtime_ns}:{st.st_size}\n".encode()
                )
        for pkl in sorted((root / "context" / "re").glob("*_funcs.pkl")):
            st = pkl.stat()
            h.update(f"p:{pkl.name}:{st.st_mtime_ns}:{st.st_size}\n".encode())
        raw = root / "context" / "re" / "raw"
        if raw.is_dir():
            n = sum(1 for p in raw.rglob("*") if p.suffix in (".gsc", ".csc"))
            h.update(f"raw:{n}\n".encode())
    return h.hexdigest()


# ---------------------------------------------------------------- rust text

_MASK_RE = re.compile(
    r"//[^\n]*"
    r"|/\*.*?\*/"
    r"|(?<![\w])r(#*)\".*?\"\1"
    r"|b?\"(?:\\.|[^\"\\])*\""
    r"|'(?:\\.[^']*|[^\\'\n])'",
    re.DOTALL,
)


def mask_comments(text: str) -> str:
    """Comments blanked to spaces (newlines kept); strings and offsets intact."""

    def blank(m: re.Match[str]) -> str:
        s = m.group(0)
        if s.startswith(("//", "/*")):
            return re.sub(r"[^\n]", " ", s)
        return s

    return _MASK_RE.sub(blank, text)


class Lines:
    def __init__(self, text: str) -> None:
        self.starts = [0]
        for m in re.finditer("\n", text):
            self.starts.append(m.end())

    def at(self, offset: int) -> int:
        lo, hi = 0, len(self.starts)
        while lo < hi:
            mid = (lo + hi) // 2
            if self.starts[mid] <= offset:
                lo = mid + 1
            else:
                hi = mid
        return lo


_CLOSE = {"(": ")", "[": "]", "{": "}"}


def balanced(text: str, start: int) -> int:
    """Offset just past the bracket that closes the one at `start`, skipping
    string literals (the text is already comment-masked)."""
    stack = []
    i, n = start, len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif c in _CLOSE:
            stack.append(_CLOSE[c])
        elif c in ")]}":
            if not stack or stack.pop() != c:
                return i + 1
            if not stack:
                return i + 1
        i += 1
    return n


_TOKEN_RE = re.compile(
    r"\s+"
    r"|(?P<str>\"(?:\\.|[^\"\\])*\")"
    r"|(?P<num>-?[0-9][0-9_]*(?:\.[0-9]+)?[a-z0-9]*)"
    r"|(?P<meta>\$[A-Za-z_]\w*(?::[a-z]+)?)"
    r"|(?P<ident>[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)"
    r"|(?P<open>[(\[{])"
    r"|(?P<punct>=>|::|[^\s\w(\[{)\]}])"
)


def tokens(text: str, base: int = 0) -> list[tuple]:
    """Flat token trees: ('str'|'num'|'meta'|'ident'|'punct', text, offset) and
    ('group', open, offset, children) for a bracketed run."""
    out: list[tuple] = []
    i, n = 0, len(text)
    while i < n:
        m = _TOKEN_RE.match(text, i)
        if not m:
            i += 1
            continue
        kind = m.lastgroup
        if kind is None:
            i = m.end()
            continue
        if kind == "open":
            end = balanced(text, i)
            out.append(
                (
                    "group",
                    text[i],
                    base + i,
                    tokens(text[i + 1 : end - 1], base + i + 1),
                )
            )
            i = end
            continue
        out.append((kind, m.group(0), base + i))
        i = m.end()
    return out


def str_value(tok: tuple) -> str | None:
    if tok[0] == "str":
        return tok[1][1:-1]
    return None


# ------------------------------------------------- macro_rules, just enough


def _pattern(toks: list[tuple]) -> list[tuple]:
    """A macro matcher: ('var', name, frag), ('rep', inner, sep, op) or ('lit', text)."""
    out: list[tuple] = []
    i = 0
    while i < len(toks):
        t = toks[i]
        if t[0] == "meta" and ":" in t[1]:
            name, frag = t[1][1:].split(":", 1)
            out.append(("var", name, frag))
        elif (
            t[0] == "punct"
            and t[1] == "$"
            and i + 1 < len(toks)
            and toks[i + 1][0] == "group"
        ):
            inner = _pattern(toks[i + 1][3])
            i += 2
            sep = None
            if i < len(toks) and toks[i][1] not in ("*", "+", "?"):
                sep = toks[i][1]
                i += 1
            op = toks[i][1] if i < len(toks) else "*"
            out.append(("rep", inner, sep, op))
        elif t[0] == "meta":
            out.append(("var", t[1][1:], "tt"))
        elif t[0] == "group":
            out.append(("lit", t[1]))
        else:
            out.append(("lit", t[1]))
        i += 1
    return out


def _match(pat: list[tuple], toks: list[tuple], pos: int, caps: list) -> int | None:
    for k, el in enumerate(pat):
        if el[0] == "lit":
            if pos >= len(toks) or toks[pos][1] != el[1]:
                return None
            pos += 1
        elif el[0] == "var":
            if pos >= len(toks):
                return None
            frag = el[2]
            if frag in ("literal", "ident", "tt", "lifetime"):
                caps.append((el[1], toks[pos]))
                pos += 1
            else:
                follow = None
                if k + 1 < len(pat) and pat[k + 1][0] == "lit":
                    follow = pat[k + 1][1]
                start = pos
                while pos < len(toks) and toks[pos][1] not in (follow, ",", ";", "=>"):
                    pos += 1
                if pos == start:
                    return None
                caps.append((el[1], toks[start]))
        else:
            _, inner, sep, op = el
            count = 0
            while True:
                trial: list = []
                end = _match(inner, toks, pos, trial)
                if end is None or end == pos:
                    break
                caps.extend(trial)
                pos = end
                count += 1
                if op == "?":
                    break
                if sep is not None:
                    if pos < len(toks) and toks[pos][1] == sep:
                        pos += 1
                    else:
                        break
            if op == "+" and count == 0:
                return None
    return pos


# ---------------------------------------------------------------- catalogs

_BUILTIN_NEW = re.compile(
    r"Builtin::new\(\s*(Function|Method)\s*,\s*\"([^\"]+)\"\s*,\s*([\w:]+)\s*,\s*(true|false)\s*,?\s*\)"
)


def scan_catalogs(root: Path) -> list[tuple]:
    rows = []
    for path in sorted((root / "crates").glob("game_*/src/*catalog*.rs")):
        game = path.parts[-3].removeprefix("game_")
        realm = "client" if path.name.startswith("client") else "server"
        text = mask_comments(path.read_text(encoding="utf-8"))
        lines = Lines(text)
        for m in _BUILTIN_NEW.finditer(text):
            rows.append(
                (
                    game,
                    realm,
                    m.group(1).lower(),
                    m.group(2).lower(),
                    m.group(3).split("::")[-1],
                    int(m.group(4) == "true"),
                    rel(root, path),
                    lines.at(m.start(2)),
                )
            )
    return rows


# ---------------------------------------------------------------- natives

_REGISTER = re.compile(
    r"\.register\(\s*(?:(?:\w+::)*Namespace::)?(Function|Method)\s*,\s*\"([^\"]+)\"\s*,"
)
_NOOP = re.compile(r"\s*\|_,\s*_,\s*_\|\s*\{?\s*Ok\(Value::Undefined\)\s*\}?\s*,?\s*\)")
_REFUSED = re.compile(
    r"\s*\|_,\s*_,\s*_\|\s*\{?\s*Err\(|\s*\|[^|]*\|\s*\{?\s*(?:[\w:]+::)?unavailable\("
)
_FOR_IN = re.compile(r"\bfor\s+&?(\w+|\([^)]*\))\s+in\s+(&?\[|&?([A-Z][A-Z0-9_]*)\b)")
_MACRO_DEF = re.compile(r"macro_rules!\s+(\w+)\s*\{")
_MOD_REGISTER = re.compile(r"\b((?:[A-Za-z_]\w*::)+)(register\w*)\b")
_FN = re.compile(r"\bfn\s+(\w+)")


def _status(text: str, after: int) -> str:
    if _NOOP.match(text, after):
        return "noop"
    if _REFUSED.match(text, after):
        return "refused"
    return "implemented"


def _module(root: Path, path: Path) -> list[str]:
    parts = list(
        path.relative_to(root / "crates" / "sim" / "src").with_suffix("").parts
    )
    if parts and parts[-1] == "mod":
        parts.pop()
    return parts


def _module_file(root: Path, parts: list[str]) -> Path | None:
    base = root / "crates" / "sim" / "src"
    for cand in (
        base.joinpath(*parts).with_suffix(".rs"),
        base.joinpath(*parts, "mod.rs"),
    ):
        if parts and cand.is_file():
            return cand
    return None


def _resolve(root: Path, current: list[str], segs: list[str]) -> Path | None:
    if segs and segs[0] in ("super", "self", "crate"):
        mod = list(current)
        for seg in segs:
            if seg == "super":
                mod = mod[:-1]
            elif seg == "crate":
                mod = []
            elif seg != "self":
                mod.append(seg)
        return _module_file(root, mod)
    for depth in range(len(current), -1, -1):
        found = _module_file(root, current[:depth] + segs)
        if found is not None:
            return found
    return None


def _fallback_scope(path: str) -> str:
    """Scope by file name when no registry constructor reaches the file."""
    name = path.rsplit("/", 1)[-1]
    if name == "t5_client.rs":
        return "t5/client"
    if "/t6/client" in path:
        return "t6/client"
    if name.startswith(("t5", "actor")):
        return "t5/server"
    if "/t6/" in path or "/t6_" in path or name.startswith("t6"):
        return "t6/server"
    if name.startswith("t7"):
        return "t7/server"
    if name.startswith("iw4"):
        return "iw4/server"
    return "shared/server"


def _scopes(root: Path, texts: dict[Path, str]) -> dict[Path, set[str]]:
    """Which games' registries reach each host file, from the constructors in
    `registry.rs` along `<module>::register…` calls."""
    edges: dict[Path, set[Path]] = defaultdict(set)
    scopes: dict[Path, set[str]] = defaultdict(set)
    registry_rs = root / HOST / "registry.rs"
    for path, text in texts.items():
        current = _module(root, path)
        fns = (
            [(m.start(), m.group(1)) for m in _FN.finditer(text)]
            if path == registry_rs
            else []
        )
        for m in _MOD_REGISTER.finditer(text):
            segs = m.group(1).rstrip(":").split("::")
            if segs[-1] in ("NativeRegistry", "Self"):
                continue
            target = _resolve(root, current, segs)
            if target is None or target not in texts:
                continue
            if path == registry_rs:
                owner = [name for at, name in fns if at < m.start()]
                scope = REGISTRY_ROOTS.get(owner[-1]) if owner else None
                if scope:
                    scopes[target].add(scope)
            else:
                edges[path].add(target)
    work = list(scopes)
    while work:
        src = work.pop()
        for dst in edges.get(src, ()):
            if not scopes[src] <= scopes[dst]:
                scopes[dst] |= scopes[src]
                work.append(dst)
    return scopes


def _array_elements(group: tuple) -> list[list[tuple]]:
    """Top-level comma-separated elements of a bracket group's tokens."""
    elems: list[list[tuple]] = [[]]
    for tok in group[3]:
        if tok[0] == "punct" and tok[1] == ",":
            elems.append([])
        else:
            elems[-1].append(tok)
    return [e for e in elems if e]


def _const_array(text: str, name: str) -> int | None:
    """Offset of the `[` opening `const NAME … = &[ … ]` in this file."""
    const = re.search(
        r"\b(?:const|static)\s+" + re.escape(name) + r"\b[^=]*=\s*&?\[", text
    )
    return const.end() - 1 if const else None


def _loop_rows(text: str, lines: Lines) -> list[tuple]:
    rows = []
    for m in _FOR_IN.finditer(text):
        pat = m.group(1)
        if m.group(3):
            arr_at = _const_array(text, m.group(3))
            if arr_at is None:
                continue
        else:
            arr_at = m.end() - 1
        arr_end = balanced(text, arr_at)
        body_at = text.find("{", m.end() if m.group(3) else arr_end)
        if body_at < 0:
            continue
        body = text[body_at : balanced(text, body_at)]
        names = [v.strip() for v in pat.strip("()").split(",")]
        reg = re.search(
            r"\.register\(\s*(?:(?:\w+::)*Namespace::)?(\w+)\s*,\s*&?(\w+)\s*,",
            body,
        )
        if not reg or reg.group(2) not in names:
            continue
        name_idx = names.index(reg.group(2))
        kind_idx = names.index(reg.group(1)) if reg.group(1) in names else None
        status = _status(body, reg.end())
        arr = tokens(text[arr_at + 1 : arr_end - 1], arr_at + 1)
        for elem in _array_elements(("group", "[", arr_at, arr)):
            parts = [elem]
            if len(names) > 1:
                if elem[0][0] != "group":
                    continue
                parts = _array_elements(elem[0])
            if name_idx >= len(parts):
                continue
            value = str_value(parts[name_idx][0])
            if value is None:
                continue
            kind = reg.group(1)
            if kind_idx is not None and kind_idx < len(parts):
                kind = parts[kind_idx][0][1].split("::")[-1]
            if kind not in ("Function", "Method"):
                continue
            rows.append(
                (kind.lower(), value.lower(), status, lines.at(parts[name_idx][0][2]))
            )
    return rows


def _macro_rows(text: str, lines: Lines) -> list[tuple]:
    defs = []
    for m in _MACRO_DEF.finditer(text):
        end = balanced(text, m.end() - 1)
        body = text[m.end() : end - 1]
        regs = {}
        for r in re.finditer(
            r"\.register\(\s*(?:(?:\w+::)*Namespace::)?(Function|Method|\$\w+)\s*,\s*\$(\w+)\s*,",
            body,
        ):
            regs[r.group(2)] = (r.group(1), _status(body, r.end()), None)
        # `register(Method, TABLE[$index].0, …)`: the name is a field of a const table row.
        for r in re.finditer(
            r"\.register\(\s*(?:(?:\w+::)*Namespace::)?(Function|Method)\s*,\s*([A-Z][A-Z0-9_]*)"
            r"\[\$(\w+)\]\.(\d+)\s*,",
            body,
        ):
            arr_at = _const_array(text, r.group(2))
            if arr_at is not None:
                arr = tokens(text[arr_at + 1 : balanced(text, arr_at) - 1], arr_at + 1)
                table = (_array_elements(("group", "[", arr_at, arr)), int(r.group(4)))
                regs[r.group(3)] = (r.group(1), _status(body, r.end()), table)
        if not regs:
            continue
        arms = []
        toks = tokens(body, m.end())
        for i, tok in enumerate(toks):
            if (
                tok[0] == "group"
                and tok[1] == "("
                and i + 1 < len(toks)
                and toks[i + 1][1] == "=>"
                and (i == 0 or toks[i - 1][1] == ";" or toks[i - 1][0] == "group")
            ):
                arms.append(_pattern(tok[3]))
        defs.append((m.start(), end, m.group(1), arms, regs))
    rows = []
    for start, end, name, arms, regs in defs:
        for inv in re.finditer(r"\b" + re.escape(name) + r"!\s*([(\[{])", text):
            if inv.start() < end:
                continue
            later = [d for d in defs if d[2] == name and end <= d[0] < inv.start()]
            if later:
                continue
            open_at = inv.end() - 1
            close = balanced(text, open_at)
            args = tokens(text[open_at + 1 : close - 1], open_at + 1)
            for arm in arms:
                caps: list = []
                if _match(arm, args, 0, caps) != len(args):
                    continue
                values = {n: t for n, t in caps if n not in regs}
                for var, tok in caps:
                    if var not in regs:
                        continue
                    kind, status, table = regs[var]
                    if table is not None and tok[0] == "num" and tok[1].isdigit():
                        elems, field = table
                        row = elems[int(tok[1])] if int(tok[1]) < len(elems) else None
                        if not row or row[0][0] != "group":
                            continue
                        parts = _array_elements(row[0])
                        if field >= len(parts):
                            continue
                        tok = parts[field][0]
                    if str_value(tok) is None:
                        continue
                    if kind.startswith("$"):
                        kind = values.get(kind[1:], ("", ""))[1].split("::")[-1]
                    if kind in ("Function", "Method"):
                        rows.append(
                            (
                                kind.lower(),
                                str_value(tok).lower(),
                                status,
                                lines.at(tok[2]),
                            )
                        )
                break
    return rows


def scan_natives(root: Path) -> list[tuple]:
    texts = {
        p: mask_comments(p.read_text(encoding="utf-8")) for p in rust_files(root, HOST)
    }
    scopes = _scopes(root, texts)
    registry_rs = root / HOST / "registry.rs"
    rows = set()
    for path, text in texts.items():
        lines = Lines(text)
        found = []
        for m in _REGISTER.finditer(text):
            found.append(
                (
                    m.group(1).lower(),
                    m.group(2).lower(),
                    _status(text, m.end()),
                    lines.at(m.start(2)),
                    m.start(),
                )
            )
        found += [(*r, -1) for r in _loop_rows(text, lines)]
        found += [(*r, -1) for r in _macro_rows(text, lines)]
        rel_path = rel(root, path)
        if path == registry_rs:
            fns = [(m.start(), m.group(1)) for m in _FN.finditer(text)]
            for kind, name, status, line, at in found:
                owner = [n for a, n in fns if a < at]
                scope = REGISTRY_ROOTS.get(owner[-1]) if owner else None
                if scope:
                    rows.add(
                        (scope, scope.split("/")[1], kind, name, status, rel_path, line)
                    )
            continue
        file_scopes = scopes.get(path) or {_fallback_scope(rel_path)}
        for scope in file_scopes:
            for kind, name, status, line, _ in found:
                rows.add(
                    (scope, scope.split("/")[1], kind, name, status, rel_path, line)
                )
    client = root / CLIENT_RS
    if client.is_file():
        text = mask_comments(client.read_text(encoding="utf-8"))
        lines = Lines(text)
        m = re.search(r"const\s+SHARED_BUILTINS\b[^=]*=", text)
        if m:
            arr_at = text.find("[", text.find("&", m.end()))
            arr_end = balanced(text, arr_at)
            for t in re.finditer(
                r"\(\s*(F|M|Function|Method)\s*,\s*\"([^\"]+)\"\s*\)",
                text[arr_at:arr_end],
            ):
                kind = "function" if t.group(1).startswith("F") else "method"
                rows.add(
                    (
                        "shared/client",
                        "client",
                        kind,
                        t.group(2).lower(),
                        "forwarded",
                        rel(root, client),
                        lines.at(arr_at + t.start(2)),
                    )
                )
    return sorted(rows)


# ---------------------------------------------------------------- ledgers

_ITEM = re.compile(r"^- \[( |x|X)\]\s*(?:\*\*([A-Z])\*\*)?\s*(.*)$")
_ID = re.compile(r"`((?:" + "|".join(GAMES) + r")\.[a-z0-9_]+(?:\.[a-z0-9_]+)*)`")
_UNKNOWN = re.compile(r"unknown!\(\s*\"([^\"]+)\"")
_ERROR_ID = re.compile(
    r"\"((?:" + "|".join(GAMES) + r")\.[a-z0-9_]+(?:\.[a-z0-9_]+)*):"
)


def scan_ledgers(root: Path) -> list[tuple]:
    rows = []
    for md in sorted((root / "docs" / "fidelity").glob("*.md")):
        game = md.stem
        lines = md.read_text(encoding="utf-8").splitlines()
        i = 0
        while i < len(lines):
            m = _ITEM.match(lines[i])
            if not m:
                i += 1
                continue
            start = i
            body = [m.group(3)]
            i += 1
            while i < len(lines) and lines[i].startswith(" ") and lines[i].strip():
                body.append(lines[i].strip())
                i += 1
            text = " ".join(body)
            summary = re.sub(r"\s+", " ", text)[:240]
            # One row per id the entry names; an entry with none keeps an empty id.
            ids = list(dict.fromkeys(_ID.findall(text))) or [""]
            for iid in ids:
                rows.append(
                    (
                        game,
                        iid,
                        m.group(2) or "",
                        int(m.group(1) != " "),
                        rel(root, md),
                        start + 1,
                        summary,
                    )
                )
    return rows


# ---------------------------------------------------------------- crates scan


def _event_role(path: str) -> str:
    crate = path.split("/")[1]
    if crate == "sim" or crate.startswith("game_"):
        return "emit"
    if crate == "net":
        return "dispatch"
    if crate.startswith("render") or crate == "audio":
        return "observe"
    if crate.startswith("entity_"):
        return "define"
    return "other"


def scan_crates(root: Path) -> dict[str, list[tuple]]:
    """ledger_code_refs, entity_events, entity_event_actions, the `net` arms that
    trigger an observer event per action, and every Bevy `On<…>` observer."""
    refs, events, actions, triggers, observers = set(), set(), set(), set(), set()
    probe = re.compile(r"unknown!\(|EntityEvent|On<|\"(?:" + "|".join(GAMES) + r")\.")
    for path in rust_files(root, "crates"):
        raw = path.read_text(encoding="utf-8", errors="replace")
        if not probe.search(raw):
            continue
        text = mask_comments(raw)
        lines = Lines(text)
        rel_path = rel(root, path)
        src_lines = raw.splitlines()
        for m in _UNKNOWN.finditer(text):
            refs.add((m.group(1), "unknown", rel_path, lines.at(m.start())))
        for m in _ERROR_ID.finditer(text):
            refs.add((m.group(1), "error", rel_path, lines.at(m.start())))
        role = _event_role(rel_path)
        for m in re.finditer(r"EntityEventKind::([A-Z][A-Z0-9_]*)\b", text):
            line = lines.at(m.start())
            detail = src_lines[line - 1].strip()[:160] if line <= len(src_lines) else ""
            if role == "dispatch":
                ahead = text[m.end() : m.end() + 200]
                d = re.match(
                    r"\s*,\s*dispatch:\s*EntityEventDispatch::(\w+)(?:\(([^)]*)\))?",
                    ahead,
                )
                if d:
                    detail = d.group(1) + (
                        f"({d.group(2).split('::')[-1]})" if d.group(2) else ""
                    )
            events.add((m.group(1), role, rel_path, line, detail))
        if role in ("observe", "dispatch"):
            for m in re.finditer(r"EntityEventAction::([A-Z]\w*)\b", text):
                line = lines.at(m.start())
                detail = (
                    src_lines[line - 1].strip()[:160] if line <= len(src_lines) else ""
                )
                actions.add((m.group(1), role, rel_path, line, detail))
        if role == "dispatch":
            for m in re.finditer(r"EntityEventAction::(\w+)\)\s*=>\s*\{", text):
                body = text[m.end() - 1 : balanced(text, m.end() - 1)]
                t = re.search(r"\.trigger\(\s*([\w:]+)\s*[{(]", body)
                if t:
                    at = m.end() - 1 + t.start(1)
                    triggers.add(
                        (m.group(1), t.group(1).split("::")[-1], rel_path, lines.at(at))
                    )
        fns = [(m.start(), m.group(1)) for m in _FN.finditer(text)]
        for m in re.finditer(r"\b\w+\s*:\s*On<\s*([\w:]+)", text):
            owner = [n for a, n in fns if a < m.start()]
            observers.add(
                (
                    m.group(1).split("::")[-1],
                    owner[-1] if owner else "",
                    rel_path,
                    lines.at(m.start()),
                )
            )
    return {
        "ledger_code_refs": sorted(refs),
        "entity_events": sorted(events),
        "entity_event_actions": sorted(actions),
        "entity_event_triggers": sorted(triggers),
        "bevy_observers": sorted(observers),
    }


# ---------------------------------------------------------------- local tables


def scan_local(root: Path) -> dict[str, list[tuple]]:
    out: dict[str, list[tuple]] = {
        "exe_builtins": [],
        "decomp_files": [],
        "call_edges": [],
        "journal_refs": [],
        "gsc_calls": [],
    }
    re_dir = re_tables_dir(root)
    if re_dir is None:
        return out
    sys.path.insert(0, str(re_dir))
    try:
        import re_tables
    finally:
        sys.path.remove(str(re_dir))
    decomp, edges = re_tables.extract_decomp_and_edges(root)
    exe = re_tables.extract_exe_builtins()
    out["exe_builtins"] = sorted(
        {(g, r, k, t, n, a, d, decomp.get(a, "")) for g, r, k, t, n, a, d in exe}
    )
    out["decomp_files"] = sorted(decomp.items())
    if hasattr(re_tables, "extract_pkl_edges"):
        edges = list(edges) + re_tables.extract_pkl_edges(root)
    out["call_edges"] = sorted(set(edges))
    out["journal_refs"] = sorted(set(re_tables.extract_journal_refs(root)))
    known = {row[4] for row in exe}
    out["gsc_calls"] = sorted(set(re_tables.extract_gsc_calls(root, known)))
    return out


# ---------------------------------------------------------------- build


def build(root: Path, db: Path, quiet: bool = False) -> None:
    fp = fingerprint(root)
    tables = {
        "catalog_builtins": scan_catalogs(root),
        "rust_natives": scan_natives(root),
        "ledger_items": scan_ledgers(root),
    }
    tables.update(scan_crates(root))
    tables.update(scan_local(root))

    db.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".port_graph.", suffix=".sqlite", dir=db.parent)
    os.close(fd)
    try:
        con = sqlite3.connect(tmp)
        con.executescript(SCHEMA)
        for table, rows in tables.items():
            if rows:
                marks = ",".join("?" * len(rows[0]))
                con.executemany(f"INSERT INTO {table} VALUES ({marks})", sorted(rows))
        meta = {
            "fingerprint": fp,
            "schema": SCHEMA_VERSION,
            "local": "yes" if re_tables_dir(root) else "no",
        }
        con.executemany("INSERT INTO meta VALUES (?, ?)", sorted(meta.items()))
        con.commit()
        con.close()
        os.chmod(tmp, 0o644)
        os.replace(tmp, db)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)
    if not quiet:
        print(f"port graph: {db}")
        for table, rows in tables.items():
            print(f"  {table:<22} {len(rows):>7}")


def stored_fingerprint(db: Path) -> str | None:
    if not db.is_file():
        return None
    try:
        con = read_only(db)
        try:
            row = con.execute(
                "SELECT value FROM meta WHERE key = 'fingerprint'"
            ).fetchone()
        finally:
            con.close()
    except sqlite3.Error:
        return None
    return row[0] if row else None


def ensure_fresh(root: Path, db: Path) -> bool:
    """Rebuild a missing or stale index; True when it rebuilt."""
    if stored_fingerprint(db) == fingerprint(root):
        return False
    build(root, db, quiet=True)
    return True


def connect(root: Path, db: Path) -> sqlite3.Connection:
    ensure_fresh(root, db)
    return read_only(db)


def read_only(db: Path) -> sqlite3.Connection:
    return sqlite3.connect(db.as_uri() + "?mode=ro", uri=True)


# ---------------------------------------------------------------- queries


def norm_addr(text: str) -> str:
    s = text.strip().lower().rsplit("_", 1)[-1]
    s = s.removeprefix("0x")
    while len(s) > 6 and s.startswith("0"):
        s = s[1:]
    return s


def show(
    label: str, rows: list, fmt, limit: int = 0, empty: str = "-", more: str = "more"
) -> None:
    if not rows:
        print(f"  {label:<10} {empty}")
        return
    shown = rows if not limit else rows[:limit]
    for i, row in enumerate(shown):
        print(f"  {label if i == 0 else '':<10} {fmt(row)}")
    if limit and len(rows) > limit:
        print(f"  {'':<10} … {len(rows) - limit} {more}")


def journal_for(con: sqlite3.Connection, tokens_: list[str], limit: int) -> None:
    if not tokens_:
        show("journal", [], str)
        return
    marks = ",".join("?" * len(tokens_))
    rows = con.execute(
        f"SELECT file, MIN(line), heading FROM journal_refs WHERE token IN ({marks}) "
        "GROUP BY file, heading ORDER BY file, MIN(line)",
        tokens_,
    ).fetchall()
    show("journal", rows, lambda r: f"{r[0]}:{r[1]}  § {r[2]}", limit)


def callees_line(con: sqlite3.Connection, addr: str, which: str = "bo") -> str:
    """Callees of `addr` in one binary (`bo` is the one the builtin tables and
    `filled/c/bo/` describe; `mp` addresses are another image)."""
    decomp = "d.path IS NOT NULL" if which == "bo" else "0"
    rows = con.execute(
        f"SELECT e.callee, {decomp}, x.name FROM call_edges e "
        "LEFT JOIN decomp_files d ON d.addr = e.callee "
        "LEFT JOIN exe_builtins x ON x.addr = e.callee AND e.which = 'bo' "
        "WHERE e.caller = ? AND e.which = ? GROUP BY e.callee ORDER BY e.callee",
        (addr, which),
    ).fetchall()
    if not rows:
        return ""
    parts = [f"{c}{'*' if done else ''}{f' ({n})' if n else ''}" for c, done, n in rows]
    return f"{len(rows)} callees (* decompiled): " + ", ".join(parts)


def cmd_builtin(con: sqlite3.Connection, names: list[str], game: str) -> None:
    local = (
        con.execute("SELECT value FROM meta WHERE key = 'local'").fetchone()[0] == "yes"
    )
    for raw in names:
        name = raw.lower()
        print(f"== {name}")
        cat = con.execute(
            "SELECT game, realm, kind, owner, dev, file, line FROM catalog_builtins "
            "WHERE name = ? ORDER BY game = ? DESC, game, realm, file",
            (name, game),
        ).fetchall()
        show(
            "catalog",
            cat,
            lambda r: (
                f"{r[0]} {r[1]} {r[2]} {r[3]}{' dev' if r[4] else ''}  {r[5]}:{r[6]}"
            ),
            empty="not in any catalog",
        )
        nat = con.execute(
            "SELECT game_scope, kind, status, file, line FROM rust_natives WHERE name = ? "
            "ORDER BY game_scope LIKE ? DESC, game_scope, file, line",
            (name, f"{game}/%"),
        ).fetchall()
        show(
            "rust",
            nat,
            lambda r: f"{r[0]} {r[1]} {r[2]}  {r[3]}:{r[4]}",
            empty="not registered",
        )
        exe = con.execute(
            "SELECT game, realm, kind, table_label, addr, dev, decomp_path FROM exe_builtins "
            "WHERE name = ? ORDER BY game, realm, table_label",
            (name,),
        ).fetchall()
        if not local:
            print("  exe        (no context/re/re_tables.py in this checkout)")
        else:
            show(
                "exe",
                exe,
                lambda r: (
                    f"{r[0]} {r[1]} {r[2]} [{r[3]}] handler {r[4]}"
                    f"{' dev' if r[5] else ''}  {r[6] or 'not decompiled'}"
                ),
                empty="in no executable table",
            )
            for r in exe:
                line = callees_line(con, r[4])
                if line:
                    print(f"  {'':<10}   {r[4]}: {line}")
        if local:
            journal_for(con, [name] + [r[4] for r in exe], 12)
            gsc = con.execute(
                "SELECT realm, file, line, snippet FROM gsc_calls WHERE name = ? "
                "ORDER BY realm, file, line",
                (name,),
            ).fetchall()
            by_realm = defaultdict(int)
            for r in gsc:
                by_realm[r[0]] += 1
            seen, picked = set(), []
            for r in gsc:
                if r[1] not in seen:
                    seen.add(r[1])
                    picked.append(r)
            if gsc:
                counts = ", ".join(f"{k} {v}" for k, v in sorted(by_realm.items()))
                print(f"  {'scripts':<10} {len(gsc)} call sites ({counts})")
            show(
                "",
                picked,
                lambda r: f"{r[1]}:{r[2]}  {r[3]}",
                8,
                empty="no call sites",
                more="more files",
            )
        print()


def cmd_addr(con: sqlite3.Connection, addrs: list[str], which: str) -> None:
    for raw in addrs:
        addr = norm_addr(raw)
        print(f"== {addr} ({which})")
        if which != "bo":
            callers = con.execute(
                "SELECT caller FROM call_edges WHERE callee = ? AND which = ? "
                "GROUP BY caller ORDER BY caller",
                (addr, which),
            ).fetchall()
            show("callers", callers, lambda r: r[0], 20)
            print(f"  {'callees':<10} {callees_line(con, addr, which) or '-'}")
            print()
            continue
        rows = con.execute(
            "SELECT game, realm, kind, table_label, name FROM exe_builtins WHERE addr = ?",
            (addr,),
        ).fetchall()
        show(
            "builtin",
            rows,
            lambda r: f"{r[4]}  ({r[0]} {r[1]} {r[2]} [{r[3]}])",
            empty="none",
        )
        path = con.execute(
            "SELECT path FROM decomp_files WHERE addr = ?", (addr,)
        ).fetchone()
        print(f"  {'decomp':<10} {path[0] if path else 'not decompiled'}")
        callers = con.execute(
            "SELECT e.caller, x.name FROM call_edges e "
            "LEFT JOIN exe_builtins x ON x.addr = e.caller "
            "WHERE e.callee = ? AND e.which = 'bo' GROUP BY e.caller ORDER BY e.caller",
            (addr,),
        ).fetchall()
        show("callers", callers, lambda r: f"{r[0]}{f' ({r[1]})' if r[1] else ''}", 20)
        line = callees_line(con, addr)
        print(f"  {'callees':<10} {line or '-'}")
        journal_for(con, [addr], 12)
        print()


def effective(con: sqlite3.Connection, game: str, realm: str) -> dict[tuple, str]:
    """(kind, name) -> status of the native the game's registry ends with:
    its own registrations override the shared ones."""
    out: dict[tuple, str] = {}
    for scope in (f"shared/{realm}", f"{game}/{realm}"):
        seen: dict[tuple, set] = defaultdict(set)
        for kind, name, status in con.execute(
            "SELECT kind, name, status FROM rust_natives WHERE game_scope = ?", (scope,)
        ):
            seen[(kind, name)].add(status)
        for key, statuses in seen.items():
            if statuses & {"implemented", "forwarded"}:
                out[key] = "implemented"
            else:
                out[key] = min(statuses)
    return out


def cmd_unimplemented(
    con: sqlite3.Connection, game: str, realm: str | None, limit: int, no_dev: bool
) -> None:
    realms = [realm] if realm else ["server", "client"]
    total = 0
    for rlm in realms:
        have = effective(con, game, rlm)
        cat = con.execute(
            "SELECT DISTINCT kind, name, owner, dev FROM catalog_builtins "
            "WHERE game = ? AND realm = ? ORDER BY name",
            (game, rlm),
        ).fetchall()
        rows = []
        for kind, name, owner, dev in cat:
            status = have.get((kind, name), "missing")
            if status == "implemented" or (no_dev and dev):
                continue
            exe = con.execute(
                "SELECT addr, decomp_path FROM exe_builtins WHERE name = ? AND realm = ? "
                "AND kind = ? ORDER BY addr LIMIT 1",
                (name, rlm, kind),
            ).fetchone()
            calls = con.execute(
                "SELECT COUNT(*) FROM gsc_calls WHERE name = ? AND realm = ?",
                (name, rlm),
            ).fetchone()[0]
            rows.append((calls, name, kind, owner, dev, status, exe))
        rows.sort(key=lambda r: (-r[0], r[1]))
        total += len(rows)
        print(
            f"== {game} {rlm}: {len(rows)} of {len(cat)} catalog builtins not implemented"
        )
        print(
            f"  {'calls':>5}  {'name':<36} {'kind':<8} {'status':<8} {'handler':<8} c"
        )
        for calls, name, kind, owner, dev, status, exe in rows[: limit or None]:
            handler = exe[0] if exe else "-"
            done = "y" if exe and exe[1] else "-"
            dev_mark = " dev" if dev else ""
            print(
                f"  {calls:>5}  {name:<36} {kind:<8} {status:<8} {handler:<8} {done}{dev_mark}"
            )
        if limit and len(rows) > limit:
            print(f"  … {len(rows) - limit} more (--limit 0 for all)")
        print()


def cmd_ledger(con: sqlite3.Connection, query: str | None) -> None:
    if not query:
        rows = con.execute(
            "SELECT game, COUNT(DISTINCT CASE WHEN checked = 0 THEN line END), "
            "COUNT(DISTINCT CASE WHEN checked = 1 THEN line END), "
            "COUNT(DISTINCT CASE WHEN checked = 0 AND id != '' THEN id END) "
            "FROM ledger_items GROUP BY game ORDER BY game"
        ).fetchall()
        print("  game   open  closed  open ids")
        for game, open_, closed, with_id in rows:
            print(f"  {game:<6} {open_:>4}  {closed:>6}  {with_id:>12}")
        orphans = con.execute(
            "SELECT DISTINCT r.id FROM ledger_code_refs r "
            "LEFT JOIN ledger_items l ON l.id = r.id WHERE l.id IS NULL ORDER BY r.id"
        ).fetchall()
        if orphans:
            print(f"  ids in code not opening a ledger entry: {len(orphans)}")
            for (oid,) in orphans[:20]:
                print(f"    {oid}")
        return
    q = f"%{query.lower()}%"
    items = con.execute(
        "SELECT game, id, severity, checked, file, line, summary FROM ledger_items "
        "WHERE lower(id) LIKE ? OR lower(summary) LIKE ? ORDER BY game, checked, line",
        (q, q),
    ).fetchall()
    seen_items = set()
    for game, iid, sev, checked, file, line, summary in items[:40]:
        box = "x" if checked else " "
        print(f"[{box}] {sev or '-'} {iid or '(no id)'}  {file}:{line}")
        if (file, line) not in seen_items:
            seen_items.add((file, line))
            print(f"      {summary[:200]}")
        if iid:
            for kind, rfile, rline in con.execute(
                "SELECT kind, file, line FROM ledger_code_refs WHERE id = ? ORDER BY file, line",
                (iid,),
            ):
                print(f"      {kind:<8} {rfile}:{rline}")
    if len(items) > 40:
        print(f"… {len(items) - 40} more")
    ids = {r[1] for r in items}
    loose = con.execute(
        "SELECT id, kind, file, line FROM ledger_code_refs WHERE lower(id) LIKE ? "
        "ORDER BY id, file, line",
        (q,),
    ).fetchall()
    loose = [r for r in loose if r[0] not in ids]
    if loose:
        print("code references with no matching ledger entry:")
        for iid, kind, file, line in loose:
            print(f"  {iid}  {kind:<8} {file}:{line}")
    if not items and not loose:
        print(f"nothing matches {query!r}")


def cmd_event(con: sqlite3.Connection, name: str | None) -> None:
    if not name:
        rows = con.execute(
            "SELECT event_name, SUM(role = 'emit'), SUM(role = 'dispatch'), SUM(role = 'observe') "
            "FROM entity_events WHERE event_name NOT IN ('NONE') "
            "GROUP BY event_name ORDER BY event_name"
        ).fetchall()
        print(f"  {'event':<34} emit dispatch observe")
        for ev, e, d, o in rows:
            print(f"  {ev:<34} {e:>4} {d:>8} {o:>7}")
        return
    ev = name.upper().removeprefix("EV_")
    print(f"== {ev}")
    for role in ("emit", "dispatch", "observe", "define", "other"):
        rows = con.execute(
            "SELECT file, line, detail FROM entity_events WHERE event_name = ? AND role = ? "
            "ORDER BY file, line",
            (ev, role),
        ).fetchall()
        if rows or role in ("emit", "dispatch", "observe"):
            show(role, rows, lambda r: f"{r[0]}:{r[1]}  {r[2]}")
    acts = set()
    for (detail,) in con.execute(
        "SELECT detail FROM entity_events WHERE event_name = ? AND role = 'dispatch'",
        (ev,),
    ):
        m = re.match(r"Observer\((\w+)\)", detail or "")
        if m:
            acts.add(m.group(1))
    for act in sorted(acts):
        trig = con.execute(
            "SELECT event_type, file, line FROM entity_event_triggers WHERE action = ? "
            "ORDER BY file, line",
            (act,),
        ).fetchall()
        show(
            f"→ {act}",
            trig,
            lambda r: f"triggers {r[0]}  {r[1]}:{r[2]}",
            empty="no trigger arm",
        )
        for event_type in sorted({r[0] for r in trig}):
            obs = con.execute(
                "SELECT fn_name, file, line FROM bevy_observers WHERE event_type = ? "
                "ORDER BY file, line",
                (event_type,),
            ).fetchall()
            show(
                "  observer",
                obs,
                lambda r, t=event_type: f"On<{t}> {r[0]}  {r[1]}:{r[2]}",
                empty="no observer",
            )


def cmd_sql(con: sqlite3.Connection, query: str) -> None:
    cur = con.execute(query)
    if cur.description is None:
        return
    cols = [d[0] for d in cur.description]
    print("\t".join(cols))
    for row in cur:
        print("\t".join("" if v is None else str(v) for v in row))


def cmd_check(root: Path, db: Path, ci: bool) -> int:
    ensure_fresh(root, db)
    con = read_only(db)
    problems = []
    for table in (
        "catalog_builtins",
        "rust_natives",
        "ledger_items",
        "entity_events",
        "entity_event_triggers",
        "bevy_observers",
    ):
        n = con.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        if n == 0:
            problems.append(
                f"{table} is empty: its parser no longer matches the source"
            )
    for table in ("catalog_builtins", "rust_natives"):
        for (file,) in con.execute(f"SELECT DISTINCT file FROM {table}"):
            if not (root / file).is_file():
                problems.append(f"{table} names a file that does not exist: {file}")
    scopes = {s for (s,) in con.execute("SELECT DISTINCT game_scope FROM rust_natives")}
    for scope in ("shared/server", "t5/server", "t5/client", "t6/server"):
        if scope not in scopes:
            problems.append(
                f"rust_natives has no {scope} natives: scope resolution broke"
            )
    con.close()
    if not ci and (root / "context" / "re").is_dir():
        local = default_db(root)
        if local != db:
            ensure_fresh(root, local)
        if stored_fingerprint(local) != fingerprint(root):
            problems.append(f"{local} is stale after a rebuild")
    for p in problems:
        print(f"port-graph: {p}", file=sys.stderr)
    if not problems:
        print(f"port-graph: ok ({db})")
    return 1 if problems else 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        prog="port_graph", description=__doc__.split("\n\n")[0]
    )
    ap.add_argument(
        "--db", type=Path, help="index path (default: context/re/port_graph.sqlite)"
    )
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("build", help="rebuild the index")
    p.add_argument("--quiet", action="store_true")
    p = sub.add_parser("check", help="rebuild if stale and verify the index")
    p.add_argument(
        "--ci", action="store_true", help="tracked inputs only; no local index"
    )
    p = sub.add_parser("builtin", help="everything known about builtins")
    p.add_argument("names", nargs="+")
    p.add_argument("--game", default="t5")
    p = sub.add_parser(
        "addr", help="a handler address: builtin, decomp, callers, journal"
    )
    p.add_argument("addrs", nargs="+")
    p.add_argument("--which", choices=("bo", "mp"), default="bo")
    p = sub.add_parser("unimplemented", help="catalog builtins with no working native")
    p.add_argument("--game", default="t5")
    p.add_argument("--realm", choices=("server", "client"))
    p.add_argument("--limit", type=int, default=0)
    p.add_argument("--no-dev", action="store_true", help="hide developer builtins")
    p = sub.add_parser("ledger", help="ledger entries and their code references")
    p.add_argument("query", nargs="?")
    p = sub.add_parser("event", help="entity event emit, dispatch and observer chains")
    p.add_argument("name", nargs="?")
    p = sub.add_parser("sql", help="read-only SQL against the index")
    p.add_argument("query")

    # `--db` is accepted after the subcommand as well.
    if "--db" in argv:
        at = argv.index("--db")
        argv = argv[at : at + 2] + argv[:at] + argv[at + 2 :]
    args = ap.parse_args(argv)
    db = (args.db or default_db(ROOT)).resolve()

    if args.cmd == "build":
        build(ROOT, db, args.quiet)
        return 0
    if args.cmd == "check":
        return cmd_check(ROOT, db, args.ci)
    con = connect(ROOT, db)
    try:
        if args.cmd == "builtin":
            cmd_builtin(con, args.names, args.game)
        elif args.cmd == "addr":
            cmd_addr(con, args.addrs, args.which)
        elif args.cmd == "unimplemented":
            cmd_unimplemented(con, args.game, args.realm, args.limit, args.no_dev)
        elif args.cmd == "ledger":
            cmd_ledger(con, args.query)
        elif args.cmd == "event":
            cmd_event(con, args.name)
        elif args.cmd == "sql":
            cmd_sql(con, args.query)
    except sqlite3.Error as error:
        print(f"port-graph: {error}", file=sys.stderr)
        return 1
    except BrokenPipeError:
        return 0
    finally:
        con.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

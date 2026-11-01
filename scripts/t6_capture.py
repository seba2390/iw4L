#!/usr/bin/env python3
"""Capture intact BO2 compiled scripts from the owner's running game, read-only."""

import argparse
import contextlib
import ctypes
import os
import struct
import sys
from pathlib import Path, PurePosixPath

MAGIC = b"\x80GSC\r\n\0\x06"
CHUNK = 1024 * 1024
LIMIT = 16 * CHUNK
TOTAL_LIMIT = 128 * CHUNK
REQUIRED = "maps/mp/gametypes_zm/_globallogic.gsc"


class InvalidModule(ValueError):
    pass


def compiled_module(read):
    cache = bytearray()

    def take(at, size):
        end = at + size
        if at < 0 or end > LIMIT:
            raise InvalidModule("module exceeds capture bounds")
        if len(cache) < end:
            more = read(len(cache), end - len(cache))
            if len(more) != end - len(cache):
                raise InvalidModule("unreadable or truncated module")
            cache.extend(more)
        return cache[at:end]

    def number(at, fmt):
        return struct.unpack(fmt, take(at, struct.calcsize(fmt)))[0]

    def word(at):
        return number(at, "<I")

    def half(at):
        return number(at, "<H")

    def text(at):
        # Names and strings use offsets in the module's 16-bit string area.
        if at < 64 or at >= 65536:
            raise InvalidModule("invalid string offset")
        end = at
        while end < 65536 and number(end, "B"):
            end += 1
        if end == 65536:
            raise InvalidModule("unterminated string")
        try:
            return bytes(take(at, end - at)).decode("utf-8")
        except UnicodeDecodeError as error:
            raise InvalidModule("invalid module string encoding") from error

    if take(0, 64)[:8] != MAGIC:
        raise InvalidModule("not a supported BO2 compiled script")
    offsets = [word(at) for at in range(12, 44, 4)]
    if any(at < 64 or at > LIMIT for at in offsets):
        raise InvalidModule("invalid table offset")
    name = text(half(48)).replace("\\", "/")
    path = PurePosixPath(name)
    if (
        not name
        or name != str(path)
        or path.is_absolute()
        or any(part in (".", "..") for part in path.parts)
        or any(char not in "abcdefghijklmnopqrstuvwxyz0123456789_./" for char in name)
        or path.suffix not in (".gsc", ".csc")
        or any(
            part.split(".")[0] in {"con", "prn", "aux", "nul"}
            or part.split(".")[0]
            in {f"{prefix}{n}" for prefix in ("com", "lpt") for n in range(1, 10)}
            for part in path.parts
        )
    ):
        raise InvalidModule("unsafe module name")

    includes, trees, code, strings, exports, imports, fixups, profiles = offsets
    code_end = code + word(44)
    take(code, code_end - code)

    def site(at, size=1):
        if at < code or at + size > code_end:
            raise InvalidModule("reference outside code")
        return take(at, size)

    def refs(at, count, size=1):
        for index in range(count):
            site(word(at + 4 * index), size)

    include_count = number(60, "B")
    for index in range(include_count):
        text(word(includes + 4 * index))
    tree_end = trees
    for _ in range(number(61, "B")):
        text(half(tree_end))
        tree_count, animation_count = half(tree_end + 2), half(tree_end + 4)
        refs(tree_end + 8, tree_count, 4)
        tree_end += 8 + 4 * tree_count
        for _ in range(animation_count):
            text(word(tree_end))
            site(word(tree_end + 4), 4)
            tree_end += 8
    string_end = strings
    for _ in range(half(50)):
        text(half(string_end))
        count = number(string_end + 2, "B")
        refs(string_end + 4, count, 2)
        string_end += 4 + 4 * count
    export_end = exports + 12 * half(52)
    for at in range(exports, export_end, 12):
        text(half(at + 8))
        entry = site(word(at + 4))[0]
        if entry != 0x17 and not (entry == 0x26 and number(at + 10, "B") == 0):
            raise InvalidModule("export has no function entry")
    import_end = imports
    opcodes = {1: 0x15, 2: 0x2E, 3: 0x32, 4: 0x30, 5: 0x34}
    for _ in range(half(54)):
        text(half(import_end))
        text(half(import_end + 2))
        count = half(import_end + 4)
        kind = number(import_end + 7, "B") & 15
        if kind not in opcodes:
            raise InvalidModule("invalid import kind")
        for index in range(count):
            at = word(import_end + 8 + 4 * index)
            size = 1 if kind == 1 else 2
            operand = site(at, size)
            if operand[0] != opcodes[kind] or (size == 2 and operand[1] != 0):
                raise InvalidModule("linked or invalid import; need an intact module")
        import_end += 8 + 4 * count
    ends = [
        includes + 4 * include_count,
        tree_end,
        code_end,
        string_end,
        export_end,
        import_end,
        fixups + 8 * half(56),
        profiles + 8 * half(58),
    ]
    length = max(ends)
    take(0, length)
    for start, end in zip(offsets, ends, strict=True):
        if start != end and end != min(
            (at for at in offsets if at > start), default=length
        ):
            raise InvalidModule("overlapping or incomplete module tables")
    # Re-read after parsing so a concurrent game load cannot produce a torn capture.
    blob = bytes(cache[:length])
    if read(0, length) != blob:
        raise InvalidModule(
            "module changed while reading; retry after loading finishes"
        )
    return name, blob


class LinuxProcess:
    def __init__(self, pid):
        self.root = Path("/proc") / str(pid)
        self.owner = self.root.stat()
        if self.owner.st_uid != os.geteuid() and os.geteuid() != 0:
            raise ValueError("capture must run as the game process's owner")
        args = (self.root / "cmdline").read_bytes().split(b"\0")
        executable = args[0].decode(errors="replace").replace("\\", "/").split("/")[-1]
        if executable.lower() != "t6zm.exe":
            raise ValueError("PID is not the owner's t6zm.exe process")
        self.memory = (self.root / "mem").open("rb", buffering=0)

    def ranges(self):
        for line in (self.root / "maps").read_text().splitlines():
            span, permissions, *_ = line.split()
            if permissions.startswith("r"):
                start, end = (int(value, 16) for value in span.split("-"))
                yield start, end

    def read(self, at, size):
        try:
            return os.pread(self.memory.fileno(), size, at)
        except OSError:
            return b""

    def close(self):
        self.memory.close()


class WindowsProcess:
    def __init__(self, pid):
        from ctypes import wintypes

        if ctypes.sizeof(ctypes.c_void_p) != 8:
            raise ValueError("use 64-bit Python to enumerate the game's memory")
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        self.api.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        self.api.OpenProcess.restype = wintypes.HANDLE
        self.api.CloseHandle.argtypes = [wintypes.HANDLE]
        self.api.QueryFullProcessImageNameW.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.DWORD),
        ]
        self.api.VirtualQueryEx.argtypes = [
            wintypes.HANDLE,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        self.api.VirtualQueryEx.restype = ctypes.c_size_t
        self.api.ReadProcessMemory.argtypes = [
            wintypes.HANDLE,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.POINTER(ctypes.c_size_t),
        ]
        self.handle = self.api.OpenProcess(0x410, False, pid)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            size = wintypes.DWORD(32768)
            name = ctypes.create_unicode_buffer(size.value)
            if not self.api.QueryFullProcessImageNameW(
                self.handle, 0, name, ctypes.byref(size)
            ):
                raise ctypes.WinError(ctypes.get_last_error())
            if Path(name.value).name.lower() != "t6zm.exe":
                raise ValueError("PID is not t6zm.exe")
        except Exception:
            self.close()
            raise

    def ranges(self):
        class MemoryInfo(ctypes.Structure):
            _fields_ = [
                ("base", ctypes.c_void_p),
                ("allocation_base", ctypes.c_void_p),
                ("allocation_protect", ctypes.c_ulong),
                ("partition", ctypes.c_ushort),
                ("size", ctypes.c_size_t),
                ("state", ctypes.c_ulong),
                ("protect", ctypes.c_ulong),
                ("kind", ctypes.c_ulong),
            ]

        at = 0
        info = MemoryInfo()
        while self.api.VirtualQueryEx(
            self.handle, at, ctypes.byref(info), ctypes.sizeof(info)
        ):
            end = (info.base or 0) + info.size
            if end <= at:
                break
            if (
                info.state == 0x1000
                and info.protect & 0xEE
                and not info.protect & 0x100
            ):
                yield info.base or 0, end
            at = end

    def read(self, at, size):
        buffer = ctypes.create_string_buffer(size)
        copied = ctypes.c_size_t()
        self.api.ReadProcessMemory(self.handle, at, buffer, size, ctypes.byref(copied))
        return buffer.raw[: copied.value]

    def close(self):
        self.api.CloseHandle(self.handle)


def scan(process, wanted=None):
    wanted = {REQUIRED} if wanted is None else wanted | {REQUIRED}
    modules = {}
    rejected = candidates = readable = total = 0
    tail = b""
    previous_end = None
    for start, end in process.ranges():
        if start != previous_end:
            tail = b""
        for at in range(start, end, CHUNK):
            size = min(CHUNK, end - at)
            block = process.read(at, size)
            readable += len(block)
            data = tail + block
            base = at - len(tail)
            offset = data.find(MAGIC)
            while offset >= 0:
                candidates += 1
                address = base + offset
                try:
                    name, blob = compiled_module(
                        lambda relative, count, address=address: process.read(
                            address + relative, count
                        )
                    )
                    if name in wanted:
                        if name in modules and modules[name] != blob:
                            raise ValueError(
                                f"conflicting intact copies of {name}; no captures saved"
                            )
                        if name not in modules:
                            total += len(blob)
                            if total > TOTAL_LIMIT:
                                raise ValueError(
                                    "capture size limit exceeded; no captures saved"
                                )
                        modules[name] = blob
                except InvalidModule:
                    rejected += 1
                offset = data.find(MAGIC, offset + 1)
            tail = data[-(len(MAGIC) - 1) :] if len(block) == size else b""
        previous_end = end
    print(
        f"Read {readable // CHUNK} MiB; {candidates} candidates, {rejected} rejected, {len(modules)} intact modules."
    )
    missing = wanted - modules.keys()
    if missing:
        raise ValueError(
            f"{', '.join(sorted(missing))} was not recovered; no captures saved. "
            "Run your original BO2 Zombies online until its launch patch has loaded, then retry. "
            "Linked/modified scripts cannot be captured by this tool. "
            "It cannot download or manufacture a missing patch."
        )
    return modules


def save(modules, destination):
    root = destination.absolute()
    paths = []
    for name, blob in sorted(modules.items()):
        path = root.joinpath(*PurePosixPath(name).parts)
        if root not in path.parents:
            raise ValueError("capture path escapes destination")
        for parent in (path, *path.parents):
            if parent.is_symlink():
                raise ValueError(f"refusing symlink in capture path: {parent}")
        if path.exists():
            if path.read_bytes() != blob:
                raise ValueError(
                    f"existing capture differs: {path}; choose a new --output folder"
                )
        else:
            paths.append((path, blob))
    created = []
    try:
        for path, blob in paths:
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open("xb") as output:
                created.append(path)
                output.write(blob)
    except BaseException:
        for path in created:
            path.unlink()
        raise
    print(
        f"Saved {len(paths)} new modules to {root}; existing identical captures preserved."
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pid",
        type=int,
        required=True,
        help="PID of your running original t6zm.exe (not IW4L)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="capture root (scripts_zm is appended); default: IW4L user folder",
    )
    parser.add_argument(
        "--module",
        action="append",
        default=[],
        help="additional compiled module path to capture; may be repeated",
    )
    args = parser.parse_args()
    if args.pid <= 0:
        parser.error("--pid must be positive")
    if args.output is None:
        if sys.platform == "win32":
            parser.error(
                "Windows needs --output pointing at your IW4L artifacts/captured folder"
            )
        if sys.platform == "linux" and os.geteuid() == 0:
            parser.error(
                "an elevated capture needs --output pointing at the owner's IW4L capture folder"
            )
        args.output = (
            Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
            / "iw4l"
            / "captured"
        )
    try:
        if sys.platform == "linux":
            process = LinuxProcess(args.pid)
        elif sys.platform == "win32":
            process = WindowsProcess(args.pid)
        else:
            raise ValueError("capture is supported on Windows and Linux/Proton only")
        with contextlib.closing(process):
            print(f"Scanning t6zm.exe PID {args.pid} (read-only)...", flush=True)
            modules = scan(process, set(args.module))
        if sys.platform == "linux" and os.geteuid() == 0 and process.owner.st_uid != 0:
            os.setgroups([])
            os.setgid(process.owner.st_gid)
            os.setuid(process.owner.st_uid)
        save(modules, args.output / "scripts_zm")
    except KeyboardInterrupt:
        print("Capture cancelled.", file=sys.stderr)
        return 130
    except (OSError, ValueError) as error:
        print(f"Capture failed: {error}", file=sys.stderr)
        if isinstance(error, PermissionError):
            print(
                "Run as the game's owner. Linux ptrace restrictions may block sibling processes; do not disable system-wide protections. See docs/BO2-ZOMBIES.md.",
                file=sys.stderr,
            )
        return 1
    print(
        "Required Zombies module recovered. Restart IW4L and try your installed map; the runtime verifies the full program."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

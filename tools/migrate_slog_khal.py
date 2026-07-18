#!/usr/bin/env python3
"""Convert k_hal serial_println! to slog_hal! with structured tags."""
from pathlib import Path
import re

ROOT = Path("crates/k_hal/src")
SKIP = {"lib.rs", "virtio.rs", "cap_gate.rs", "discovery.rs"}

# Match: serial_println!("..."); or with format args
LINE_RE = re.compile(
    r'^(\s*)serial_println!\(\s*"(.*?)"(.*)\);\s*$'
)


def split_tag(msg: str):
    """Return (item, sub, body) from legacy prefix like [GPU-BAR] text."""
    msg = msg.strip()
    if msg.startswith("["):
        end = msg.find("]")
        if end > 0:
            tag = msg[1:end]
            body = msg[end + 1 :].lstrip(" -—:\t")
            tag = tag.replace("K-HAL-", "").replace("GPU-", "GPU/")
            if "/" in tag:
                a, b = tag.split("/", 1)
                return a[:32], b[:32], body
            if "-" in tag:
                parts = tag.split("-", 1)
                return parts[0][:32], parts[1][:32], body
            return tag[:32], "info", body
    if ":" in msg[:40]:
        left, right = msg.split(":", 1)
        return left.strip()[:32], "info", right.strip()
    return "Log", "msg", msg


def convert_file(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    if "serial_println!" not in text:
        return 0
    lines = text.splitlines(keepends=True)
    out = []
    n = 0
    for line in lines:
        m = LINE_RE.match(line.rstrip("\n\r"))
        if not m:
            out.append(line)
            continue
        indent, msg, rest = m.group(1), m.group(2), m.group(3)
        item, sub, body = split_tag(msg)
        # rest is like , a, b)  or )
        rest = rest.strip()
        if rest.startswith(","):
            fmt_args = rest  # includes leading comma through closing paren content
            # rest was ", args)" — keep ", args"
            if fmt_args.endswith(")"):
                fmt_args = fmt_args[:-1]
            new = f'{indent}k_nano::slog_hal!("{item}", "{sub}", "{body}"{fmt_args});\n'
        else:
            new = f'{indent}k_nano::slog_hal!("{item}", "{sub}", "{body}");\n'
        out.append(new)
        n += 1
    if n:
        # drop unused serial_println import
        joined = "".join(out)
        joined = joined.replace("use k_nano::serial_println;\n", "")
        path.write_text(joined, encoding="utf-8")
    return n


def main():
    total = 0
    for p in sorted(ROOT.rglob("*.rs")):
        if p.name in SKIP:
            continue
        c = convert_file(p)
        if c:
            print(f"{p}: {c}")
            total += c
    print("TOTAL", total)


if __name__ == "__main__":
    main()

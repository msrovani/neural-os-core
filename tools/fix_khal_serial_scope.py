#!/usr/bin/env python3
"""Prefix bare serial_println! with k_nano:: in k_hal; convert simple single-line to slog_hal."""
from pathlib import Path
import re

ROOT = Path("crates/k_hal/src")

SIMPLE = re.compile(
    r'^(\s*)(?:k_nano::)?serial_println!\(\s*"(.*?)"\s*\);\s*$'
)


def split_tag(msg: str):
    msg = msg.strip()
    if msg.startswith("["):
        end = msg.find("]")
        if end > 0:
            tag = msg[1:end]
            body = msg[end + 1 :].lstrip(" -—:\t")
            tag = tag.replace("K-HAL-", "")
            if "-" in tag:
                a, b = tag.split("-", 1)
                return a[:32], b[:32], body
            return tag[:32], "info", body
    return "Log", "msg", msg


def process(path: Path):
    text = path.read_text(encoding="utf-8")
    orig = text
    # bare serial_println! -> k_nano::serial_println!
    text = re.sub(r'(?<![:\w])serial_println!', "k_nano::serial_println!", text)

    lines = text.splitlines(keepends=True)
    out = []
    for line in lines:
        m = SIMPLE.match(line.rstrip("\n\r"))
        if m and "slog_hal!" not in line:
            indent, msg = m.group(1), m.group(2)
            # only convert if no format placeholders needing trailing args — all in string
            if "{" not in msg or msg.count("{") == msg.count("}"):
                # if has {} it's still ok for format_args with no extra args (literal braces rare)
                if "{" in msg and "}" in msg:
                    # likely needs args — leave as serial for multiline safety
                    out.append(line if line.startswith(indent + "k_nano::") else line)
                    # keep prefixed serial
                    if not line.lstrip().startswith("k_nano::serial_println"):
                        out[-1] = re.sub(
                            r'(?<![:\w])serial_println!',
                            "k_nano::serial_println!",
                            line,
                        )
                    continue
                item, sub, body = split_tag(msg)
                out.append(
                    f'{indent}k_nano::slog_hal!("{item}", "{sub}", "{body}");\n'
                )
                continue
        out.append(line)
    new = "".join(out)
    if new != orig:
        path.write_text(new, encoding="utf-8")
        return True
    return False


def main():
    n = 0
    for p in ROOT.rglob("*.rs"):
        if process(p):
            print("updated", p)
            n += 1
    print("files", n)


if __name__ == "__main__":
    main()

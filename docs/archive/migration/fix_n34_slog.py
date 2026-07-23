#!/usr/bin/env python3
"""Replace N3/N4 multiline gate serial_println with slog_*."""
from pathlib import Path
import re

p = Path("crates/neural-kernel/src/main.rs")
t = p.read_text(encoding="utf-8")

# Convert remaining multiline: serial_println!( "\n [TAG] fmt", args);
# Already partially done. Convert any leftover [N4-HERMES] / [N3-CORTEX] strings.

def repl_tag(tag, macro, item, sub):
    global t
    # single-line leftovers
    pat = re.compile(
        rf'serial_println!\(\s*"\[{re.escape(tag)}\] ([^"]*)"(,*)([^;]*)\);',
        re.M,
    )
    def f(m):
        fmt, comma, args = m.group(1), m.group(2), m.group(3)
        return f'k_nano::{macro}!("{item}", "{sub}", "{fmt}"{comma}{args});'
    t2, n = pat.subn(f, t)
    t = t2
    print(tag, "single", n)

    # multiline form
    pat2 = re.compile(
        rf'serial_println!\(\s*\n\s*"\[{re.escape(tag)}\] ([^"]*)"\s*,\s*\n((?:\s*.*\n)*?)\s*\);',
        re.M,
    )
    def f2(m):
        fmt, args_block = m.group(1), m.group(2)
        return f'k_nano::{macro}(\n        "{item}",\n        "{sub}",\n        "{fmt}",\n{args_block}    );'
    t2, n2 = pat2.subn(f2, t)
    t = t2
    print(tag, "multi", n2)

repl_tag("N4-HERMES", "slog_hermes!", "Gate", "n4")
repl_tag("N3-CORTEX", "slog_cortex!", "Gate", "n3")
repl_tag("N2-SELFHEAL", "slog_kai!", "Gate", "n2")

p.write_text(t, encoding="utf-8")
print("done")

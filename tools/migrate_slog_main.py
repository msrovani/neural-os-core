#!/usr/bin/env python3
"""Migrate neural-kernel main.rs boot/gate logs to slog_* macros."""
from pathlib import Path
import re

path = Path("crates/neural-kernel/src/main.rs")
text = path.read_text(encoding="utf-8")

# BOOT Registering X...
def boot_reg(m):
    indent = m.group(1)
    name = m.group(2).rstrip(".")
    return f'{indent}k_nano::slog_bin!("Boot", "register", "{name}");'

text2 = re.sub(
    r'^(\s*)serial_println!\("\[BOOT\] Registering ([^"]+)"\);',
    boot_reg,
    text,
    flags=re.M,
)

# Simple [TAG] msg without format args
def simple_gate(m):
    indent, tag, body = m.group(1), m.group(2), m.group(3)
    mapping = {
        "N5-JARBAS": ("slog_jarbas!", "Gate", "n5"),
        "N4-HERMES": ("slog_hermes!", "Gate", "n4"),
        "N3-CORTEX": ("slog_cortex!", "Gate", "n3"),
        "N2-SELFHEAL": ("slog_kai!", "Gate", "n2"),
        "SCHEDULER": ("slog_bin!", "Sched", "start"),
        "GPU": ("slog_hal!", "GPU", "info"),
        "FW": ("slog_hal!", "FW", "info"),
        "PIPER": ("slog_bin!", "Asset", "piper"),
        "BGE": ("slog_bin!", "Asset", "bge"),
        "STATUS": ("slog_bin!", "Asset", "status"),
        "P4": ("slog_bin!", "Cap", "p4"),
        "P3": ("slog_bin!", "Cap", "p3"),
    }
    macro, item, sub = mapping.get(tag, ("slog_bin!", tag[:24], "info"))
    return f'{indent}k_nano::{macro}("{item}", "{sub}", "{body}");'

text3 = re.sub(
    r'^(\s*)serial_println!\("\[([A-Z0-9-]+)\] ([^"{}]+)"\);',
    simple_gate,
    text2,
    flags=re.M,
)

# Format-string gates: serial_println!("[N5-JARBAS] ... {}", ...)
def fmt_gate(m):
    indent, tag, fmt, args = m.group(1), m.group(2), m.group(3), m.group(4)
    mapping = {
        "N5-JARBAS": ("slog_jarbas!", "Gate", "n5"),
        "N4-HERMES": ("slog_hermes!", "Gate", "n4"),
        "N3-CORTEX": ("slog_cortex!", "Gate", "n3"),
        "N2-SELFHEAL": ("slog_kai!", "Gate", "n2"),
        "SCHEDULER": ("slog_bin!", "Sched", "info"),
        "GPU": ("slog_hal!", "GPU", "info"),
        "BOOT": ("slog_bin!", "Boot", "info"),
        "AGENT": ("slog_bin!", "Agent", "info"),
        "NET": ("slog_hermes!", "Net", "info"),
        "LLM": ("slog_cortex!", "LLM", "info"),
        "RAMDISK": ("slog_bin!", "Asset", "ramdisk"),
        "FAT": ("slog_nano!", "FAT", "info"),
        "VGPU": ("slog_jarbas!", "VGPU", "info"),
    }
    macro, item, sub = mapping.get(tag, ("slog_bin!", tag[:24], "info"))
    return f'{indent}k_nano::{macro}("{item}", "{sub}", "{fmt}"{args});'

text4 = re.sub(
    r'^(\s*)serial_println!\("\[([A-Z0-9-]+)\] ([^"]*)"(,*[^;]*)\);',
    fmt_gate,
    text3,
    flags=re.M,
)

if text4 != text:
    path.write_text(text4, encoding="utf-8")
    print("updated main.rs", "delta chars", abs(len(text4) - len(text)))
else:
    print("no change")

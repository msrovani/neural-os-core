#!/usr/bin/env python3
"""Migrate serial_println! → slog_* across K²CHJ crates (multiline-aware).

Formato: [Rn] [k-xxx] [Item] [subitem] - body

Uso:
  python tools/migrate_slog_all.py              # todos os crates mapeados
  python tools/migrate_slog_all.py k_hal hermes # subset
  python tools/migrate_slog_all.py --dry-run
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# crate dir → (macro_name, skip_files)
CRATE_MAP = {
    "k_hal": ("slog_hal!", set()),
    "k_nano": ("slog_nano!", {"serial.rs", "slog.rs"}),
    "k_ai": ("slog_kai!", set()),
    "cortex": ("slog_cortex!", set()),
    "hermes": ("slog_hermes!", set()),
    "jarbas": ("slog_jarbas!", set()),
    "neural-kernel": ("slog_bin!", set()),
}

# Tag heuristics → (Item, subitem). Prefer explicit boot/gate tags.
TAG_MAP = {
    "K-HAL": ("HAL", "info"),
    "K-HAL-VIRTIO": ("VirtIO", "info"),
    "K-HAL-CAP": ("Cap", "info"),
    "K-HAL-AS": ("AS", "info"),
    "SCL": ("SCL", "map"),
    "N5-JARBAS": ("Gate", "n5"),
    "N4-HERMES": ("Gate", "n4"),
    "N3-CORTEX": ("Gate", "n3"),
    "N2-SELFHEAL": ("Gate", "n2"),
    "N1": ("Gate", "n1"),
    "BOOT": ("Boot", "info"),
    "DBG3": ("Boot", "dbg"),
    "DBG4": ("Boot", "dbg"),
    "DBG4b": ("Boot", "dbg"),
    "DBG6": ("Boot", "dbg"),
    "SCHEDULER": ("Sched", "info"),
    "AGENT": ("Agent", "info"),
    "AGENTS": ("Agent", "info"),
    "MONITOR": ("Agent", "monitor"),
    "INPUT": ("Input", "info"),
    "SYS": ("Sys", "info"),
    "HERMES": ("Hermes", "info"),
    "Hermes": ("Hermes", "info"),
    "Hermes-PnP": ("PnP", "info"),
    "CORTEX-LLM": ("LLM", "info"),
    "LLM": ("LLM", "info"),
    "SKILL-LLM": ("Skill", "llm"),
    "WORKFLOW": ("Workflow", "info"),
    "SECURITY": ("Sec", "info"),
    "SEC": ("Sec", "info"),
    "HEALTH": ("Health", "info"),
    "VFS": ("VFS", "info"),
    "FAT": ("FAT", "info"),
    "AHCI": ("Disk", "ahci"),
    "ATA": ("Disk", "ata"),
    "NVME": ("Disk", "nvme"),
    "USB-MSC": ("USB", "msc"),
    "USB": ("USB", "info"),
    "XHCI": ("USB", "xhci"),
    "NET": ("Net", "info"),
    "E1000": ("Net", "e1000"),
    "RTL8139": ("Net", "rtl8139"),
    "VIRTIO-NET": ("Net", "virtio"),
    "WIFI": ("Wifi", "info"),
    "WIFI-DMA": ("Wifi", "dma"),
    "GPU": ("GPU", "info"),
    "GPU-RING": ("GPU", "ring"),
    "GPU-BAR": ("GPU", "bar"),
    "NVIDIA": ("GPU", "nvidia"),
    "INTEL": ("GPU", "intel"),
    "AMD": ("GPU", "amd"),
    "CANARY": ("GPU", "canary"),
    "BENCH": ("GPU", "bench"),
    "SECURE-BOOT": ("GPU", "secureboot"),
    "GTT": ("GPU", "gtt"),
    "KV-DMA": ("GPU", "kvdma"),
    "FW": ("FW", "info"),
    "VGPU": ("VGPU", "info"),
    "PIPER": ("Audio", "piper"),
    "HDA": ("Audio", "hda"),
    "STT": ("Audio", "stt"),
    "TTS": ("Audio", "tts"),
    "AUDIO": ("Audio", "info"),
    "WAKE": ("Audio", "wake"),
    "JARVIS": ("Jarbas", "info"),
    "FB": ("Display", "fb"),
    "DISPLAY": ("Display", "info"),
    "SMP": ("SMP", "info"),
    "APIC": ("APIC", "info"),
    "PCI": ("PCI", "info"),
    "ACPI": ("ACPI", "info"),
    "WARN": ("Warn", "info"),
    "ERROR": ("Error", "info"),
    "STATUS": ("Status", "info"),
    "RAMDISK": ("Asset", "ramdisk"),
    "BGE": ("Asset", "bge"),
    "P3": ("Cap", "p3"),
    "P4": ("Cap", "p4"),
    "P8": ("Cap", "p8"),
    "CAP": ("Cap", "info"),
    "TRUST": ("Trust", "info"),
    "SELFHEAL": ("SelfHeal", "info"),
    "OPTIMIZER": ("Optimizer", "info"),
    "CRON": ("Cron", "info"),
    "MCP": ("MCP", "info"),
    "WASM": ("Wasm", "info"),
    "MARKET": ("Market", "info"),
    "PACKAGE": ("Package", "info"),
    "NFS": ("NeuralFS", "info"),
    "NEURAL-FS": ("NeuralFS", "info"),
    "RL": ("Boot", "rl"),
}


def sanitize_ident(s: str, maxlen: int = 32) -> str:
    s = re.sub(r"[^A-Za-z0-9_-]+", "_", s.strip())
    s = s.strip("_") or "Log"
    return s[:maxlen]


def split_tag(fmt: str) -> tuple[str, str, str]:
    """Extract (item, sub, body) from format string."""
    fmt = fmt.strip()
    m = re.match(r"^\[([^\]]+)\]\s*(.*)$", fmt, re.S)
    if m:
        tag, body = m.group(1), m.group(2).lstrip(" -—:\t")
        # Dynamic tag like [{}] or [NET @t={}] — keep full fmt as body
        if "{" in tag:
            return "Log", "dyn", fmt
        if tag in TAG_MAP:
            item, sub = TAG_MAP[tag]
            return item, sub, body
        # K-HAL-FOO / GPU-BAR style
        clean = tag.replace("K-HAL-", "").replace("K-NANO-", "")
        if "-" in clean:
            a, b = clean.split("-", 1)
            return sanitize_ident(a), sanitize_ident(b), body
        if "/" in clean:
            a, b = clean.split("/", 1)
            return sanitize_ident(a), sanitize_ident(b), body
        return sanitize_ident(clean), "info", body
    # "PREFIX: rest"
    if ":" in fmt[:48] and "{" not in fmt[: fmt.find(":")]:
        left, right = fmt.split(":", 1)
        if len(left) < 28 and " " not in left.strip():
            return sanitize_ident(left), "info", right.strip()
    return "Log", "msg", fmt


def find_macro_calls(text: str) -> list[tuple[int, int, str]]:
    """Return list of (start, end, full_match) for serial_println! invocations."""
    results = []
    # Match optional path prefix + serial_println!
    pat = re.compile(
        r"(?:(?:k_nano|crate|super|self)::)*serial_println!\s*\(",
        re.M,
    )
    for m in pat.finditer(text):
        start = m.start()
        i = m.end()  # after '('
        depth = 1
        in_str = False
        escape = False
        while i < len(text) and depth > 0:
            c = text[i]
            if in_str:
                if escape:
                    escape = False
                elif c == "\\":
                    escape = True
                elif c == '"':
                    in_str = False
            else:
                if c == '"':
                    in_str = True
                elif c == "(":
                    depth += 1
                elif c == ")":
                    depth -= 1
            i += 1
        # leave trailing `;` outside the span (we re-emit it)
        end = i
        results.append((start, end, text[start:end]))
    return results


def parse_call(call: str) -> tuple[str, str] | None:
    """Return (fmt_raw_inside_quotes, args_after_fmt) — fmt keeps Rust escapes."""
    open_paren = call.find("(")
    if open_paren < 0:
        return None
    inner = call[open_paren + 1 :].rstrip()
    if inner.endswith(")"):
        inner = inner[:-1]
    inner = inner.strip()
    if not inner.startswith('"'):
        return None
    i = 1
    while i < len(inner):
        c = inner[i]
        if c == "\\" and i + 1 < len(inner):
            i += 2
            continue
        if c == '"':
            fmt = inner[1:i]  # raw escapes as in source
            rest = inner[i + 1 :].strip()
            if rest.startswith(","):
                rest = rest[1:].strip()
            return fmt, rest
        i += 1
    return None


# neural-kernel gate tags → better ring macros
GATE_MACRO = {
    "N5-JARBAS": "slog_jarbas!",
    "N4-HERMES": "slog_hermes!",
    "N3-CORTEX": "slog_cortex!",
    "N2-SELFHEAL": "slog_kai!",
    "CORTEX-LLM": "slog_cortex!",
    "LLM": "slog_cortex!",
    "HERMES": "slog_hermes!",
    "Hermes": "slog_hermes!",
    "NET": "slog_hermes!",
    "WIFI": "slog_hermes!",
    "GPU": "slog_hal!",
    "NVIDIA": "slog_hal!",
    "INTEL": "slog_hal!",
    "AMD": "slog_hal!",
    "VGPU": "slog_jarbas!",
    "JARVIS": "slog_jarbas!",
    "FB": "slog_jarbas!",
    "DISPLAY": "slog_jarbas!",
    "HDA": "slog_jarbas!",
    "PIPER": "slog_jarbas!",
    "FAT": "slog_nano!",
    "AHCI": "slog_nano!",
    "ATA": "slog_nano!",
    "PCI": "slog_nano!",
    "APIC": "slog_nano!",
    "SMP": "slog_nano!",
    "XHCI": "slog_nano!",
    "USB-MSC": "slog_nano!",
}


def convert_call(call: str, macro: str, prefix: str = "k_nano") -> str | None:
    parsed = parse_call(call)
    if parsed is None:
        return None
    fmt, args = parsed
    fmt_plain = (
        fmt.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace('\\"', '"')
        .replace("\\\\", "\\")
    )
    item, sub, body = split_tag(fmt_plain)
    # Override macro from leading [TAG] when present
    m = re.match(r"^\[([^\]]+)\]", fmt_plain)
    if m and m.group(1) in GATE_MACRO:
        macro = GATE_MACRO[m.group(1)]
    body_esc = (
        body.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )
    path = f"{prefix}::{macro}"
    if args:
        if "\n" in args or len(args) > 60:
            return f'{path}("{item}", "{sub}", "{body_esc}",\n{args})'
        return f'{path}("{item}", "{sub}", "{body_esc}", {args})'
    return f'{path}("{item}", "{sub}", "{body_esc}")'


def line_indent_at(text: str, pos: int) -> str:
    line_start = text.rfind("\n", 0, pos) + 1
    indent = []
    for c in text[line_start:pos]:
        if c in " \t":
            indent.append(c)
        else:
            break
    return "".join(indent)


def convert_file(path: Path, macro: str, dry_run: bool, prefix: str = "k_nano") -> int:
    text = path.read_text(encoding="utf-8")
    if "serial_println!" not in text:
        return 0
    calls = find_macro_calls(text)
    if not calls:
        return 0
    # replace from end to start so offsets stay valid
    n = 0
    new_text = text
    for start, end, call in reversed(calls):
        repl = convert_call(call, macro, prefix=prefix)
        if repl is None:
            continue
        indent = line_indent_at(new_text, start)
        if "\n" in repl:
            parts = repl.split("\n")
            repl = parts[0] + "\n" + "\n".join(
                indent + "    " + p.lstrip() for p in parts[1:]
            )
        # eat optional trailing semicolon from source; only re-emit if present
        eat = end
        while eat < len(new_text) and new_text[eat] in " \t":
            eat += 1
        had_semi = eat < len(new_text) and new_text[eat] == ";"
        if had_semi:
            eat += 1
        new_text = new_text[:start] + repl + (";" if had_semi else "") + new_text[eat:]
        n += 1
    if n and not dry_run:
        new_text = re.sub(
            r"^use (?:k_nano::)?(?:crate::)?serial_println;\s*\n",
            "",
            new_text,
            flags=re.M,
        )
        path.write_text(new_text, encoding="utf-8")
    return n


def process_crate(name: str, dry_run: bool) -> int:
    if name not in CRATE_MAP:
        print(f"skip unknown crate {name}", file=sys.stderr)
        return 0
    macro, skip = CRATE_MAP[name]
    prefix = "crate" if name == "k_nano" else "k_nano"
    src = ROOT / "crates" / name / "src"
    if not src.is_dir():
        print(f"missing {src}", file=sys.stderr)
        return 0
    total = 0
    for p in sorted(src.rglob("*.rs")):
        if p.name in skip:
            continue
        c = convert_file(p, macro, dry_run, prefix=prefix)
        if c:
            print(f"{'DRY ' if dry_run else ''}{p.relative_to(ROOT)}: {c}")
            total += c
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("crates", nargs="*", help="crate names (default: all mapped)")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    crates = args.crates or list(CRATE_MAP.keys())
    grand = 0
    for c in crates:
        grand += process_crate(c, args.dry_run)
    print(f"{'DRY ' if args.dry_run else ''}TOTAL {grand}")


if __name__ == "__main__":
    main()

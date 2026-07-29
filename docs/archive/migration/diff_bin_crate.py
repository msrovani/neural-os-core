#!/usr/bin/env python3
"""diff_bin_crate.py — Onda 0 gate for neural-kernel thinning.

Compares dual-copy modules between neural-kernel (bin) and K³CHJ crates.
Fails (--strict) if a listed cutover target is bin_ahead (would lose code).

Usage:
  python tools/diff_bin_crate.py
  python tools/diff_bin_crate.py --onda 1 --strict
  python tools/diff_bin_crate.py --markdown docs/memory/BIN_CRATE_DIFF.md
"""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BIN = ROOT / "crates" / "neural-kernel" / "src"

# (bin_relpath, crate_root_relpath, crate_relpath, onda, notes)
PAIRS: list[tuple[str, str, str, int, str]] = [
    # Already cutover (thin stubs) — onda 0 reference
    ("identity.rs", "crates/k_nano/src", "identity.rs", 0, "already stub"),
    ("memory.rs", "crates/k_nano/src", "memory.rs", 0, "already stub"),
    ("mhi.rs", "crates/k_nano/src", "mhi.rs", 0, "already stub"),
    ("agency.rs", "crates/k_ai/src", "agency.rs", 0, "already stub"),
    ("audit.rs", "crates/k_ai/src", "audit.rs", 0, "already stub"),
    # Onda 1 — low-risk k_nano clones
    ("sync", "crates/k_nano/src", "sync", 1, "dir"),
    ("gpt.rs", "crates/k_nano/src", "gpt.rs", 1, ""),
    ("exfat.rs", "crates/k_nano/src", "exfat.rs", 1, ""),
    ("exfat_write.rs", "crates/k_nano/src", "exfat_write.rs", 1, ""),
    ("tpm.rs", "crates/k_nano/src", "tpm.rs", 1, ""),
    ("hw_rng.rs", "crates/k_nano/src", "hw_rng.rs", 1, ""),
    ("slip.rs", "crates/k_nano/src", "slip.rs", 1, ""),
    ("dma.rs", "crates/k_nano/src", "dma.rs", 1, ""),
    ("slab.rs", "crates/k_nano/src", "slab.rs", 1, ""),
    ("io_scheduler.rs", "crates/k_nano/src", "io_scheduler.rs", 1, ""),
    ("fs_driver.rs", "crates/k_nano/src", "fs_driver.rs", 1, ""),
    ("storage_manager.rs", "crates/k_nano/src", "storage_manager.rs", 1, ""),
    ("rtl8139.rs", "crates/k_nano/src", "rtl8139.rs", 1, ""),
    ("ahci.rs", "crates/k_nano/src", "ahci.rs", 1, "AHCI_DRIVER in bin main"),
    # Onda 2 — k_ai thin
    ("conversation.rs", "crates/k_ai/src", "conversation.rs", 2, ""),
    ("hw_agents.rs", "crates/k_ai/src", "hw_agents.rs", 2, ""),
    ("cognitive.rs", "crates/k_ai/src", "cognitive.rs", 2, ""),
    ("chunker.rs", "crates/k_ai/src", "chunker.rs", 2, ""),
    ("usage.rs", "crates/k_ai/src", "usage.rs", 2, ""),
    ("profile.rs", "crates/k_ai/src", "profile.rs", 2, ""),
    ("context_window.rs", "crates/k_ai/src", "context_window.rs", 2, ""),
    ("training_agent.rs", "crates/k_ai/src", "training_agent.rs", 2, ""),
    ("memory_agent.rs", "crates/k_ai/src", "memory_agent.rs", 2, ""),
    ("boot_log_agent.rs", "crates/k_ai/src", "boot_log_agent.rs", 2, ""),
    ("shutdown.rs", "crates/k_ai/src", "shutdown.rs", 2, "split: HW stays bin"),
    ("inventory.rs", "crates/k_ai/src", "inventory.rs", 2, "align HAL bridge"),
    ("gguf.rs", "crates/k_ai/src", "gguf.rs", 2, "promote bin first"),
    # Onda 3 — k_nano mid
    ("pci.rs", "crates/k_nano/src", "pci.rs", 3, ""),
    ("serial.rs", "crates/k_nano/src", "serial.rs", 3, ""),
    ("vga_buffer.rs", "crates/k_nano/src", "vga_buffer.rs", 3, "macros"),
    ("xhci.rs", "crates/k_nano/src", "xhci.rs", 3, ""),
    ("usb_msc.rs", "crates/k_nano/src", "usb_msc.rs", 3, ""),
    ("virtio_net.rs", "crates/k_nano/src", "virtio_net.rs", 3, ""),
    ("block_dev.rs", "crates/k_nano/src", "block_dev.rs", 3, ""),
    ("simd.rs", "crates/k_nano/src", "simd.rs", 3, ""),
    # Onda 4 — disk
    ("fat32.rs", "crates/k_nano/src", "fat32.rs", 4, "promote helpers"),
    ("ata.rs", "crates/k_nano/src", "ata.rs", 4, "unify ATA_DRIVER"),
    ("e1000.rs", "crates/k_nano/src", "e1000.rs", 4, "promote prove_rx"),
    ("neural_fs", "crates/k_nano/src", "neural_fs", 4, "dir; promote agent"),
    # Onda 5 — platform
    ("acpi.rs", "crates/k_nano/src", "acpi.rs", 5, "RSDP unique"),
    ("apic.rs", "crates/k_nano/src", "apic.rs", 5, "LAPIC unique"),
    ("smp", "crates/k_nano/src", "smp", 5, "dir"),
    ("interrupts.rs", "crates/k_nano/src", "interrupts.rs", 5, "TIMER_TICKS unique"),
    ("boot_logger.rs", "crates/k_nano/src", "boot_logger.rs", 5, "promote bin"),
    # Onda 6 — residuals
    ("global_arena.rs", "crates/cortex/src", "global_arena.rs", 6, "pending_route"),
    ("model_hub.rs", "crates/cortex/src", "model_hub.rs", 6, "bin truth"),
    ("bpe.rs", "crates/cortex/src", "bpe.rs", 6, "API incompatible"),
    ("cortex.rs", "crates/cortex/src", "cortex.rs", 6, "boot LLM path"),
    ("agents.rs", "crates/hermes/src", "agents.rs", 6, "fleet"),
    ("aios_api.rs", "crates/hermes/src", "aios_api.rs", 6, ""),
    ("micropython_wasm.rs", "crates/hermes/src", "micropython_wasm.rs", 6, ""),
    ("net.rs", "crates/k_nano/src", "net.rs", 6, "role_diff; keep bridge"),
]


def is_stub(path: Path) -> bool:
    if not path.is_file():
        return False
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = [
        ln.strip()
        for ln in text.splitlines()
        if ln.strip() and not ln.strip().startswith("//") and not ln.strip().startswith("//!")
    ]
    if not lines:
        return False
    return all(ln.startswith("pub use ") for ln in lines)


def collect_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root]
    if not root.is_dir():
        return []
    return sorted(p for p in root.rglob("*.rs") if p.is_file())


def file_hash(path: Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()[:16]


def normalize_rs(text: str) -> str:
    """Strip comments and blank lines for soft compare."""
    out = []
    for ln in text.splitlines():
        s = ln.strip()
        if not s or s.startswith("//"):
            continue
        out.append(s)
    return "\n".join(out)


def loc(path: Path) -> int:
    if path.is_dir():
        return sum(loc(p) for p in collect_files(path))
    try:
        return sum(1 for _ in path.open(encoding="utf-8", errors="replace"))
    except OSError:
        return 0


def classify(bin_path: Path, crate_path: Path) -> tuple[str, str]:
    if is_stub(bin_path):
        return "stub", "already pub use"
    if not bin_path.exists():
        return "missing_bin", "bin path absent"
    if not crate_path.exists():
        return "missing_crate", "crate path absent"

    bin_files = collect_files(bin_path)
    crate_files = collect_files(crate_path)
    if not bin_files or not crate_files:
        return "missing", "empty tree"

    # Single-file exact
    if bin_path.is_file() and crate_path.is_file():
        bh = file_hash(bin_path)
        ch = file_hash(crate_path)
        if bh == ch:
            return "identical", f"sha={bh}"
        bn = normalize_rs(bin_path.read_text(encoding="utf-8", errors="replace"))
        cn = normalize_rs(crate_path.read_text(encoding="utf-8", errors="replace"))
        if bn == cn:
            return "identical_norm", "whitespace/comment only"
        bl, cl = loc(bin_path), loc(crate_path)
        if bl > cl + 20:
            return "bin_ahead", f"loc {bl}>{cl}"
        if cl > bl + 20:
            return "crate_ahead", f"loc {cl}>{bl}"
        return "diverged", f"loc bin={bl} crate={cl}"

    # Directory: compare relative names + hashes
    def relmap(root: Path, files: list[Path]) -> dict[str, Path]:
        return {str(p.relative_to(root)).replace("\\", "/"): p for p in files}

    bm, cm = relmap(bin_path, bin_files), relmap(crate_path, crate_files)
    only_bin = sorted(set(bm) - set(cm))
    only_crate = sorted(set(cm) - set(bm))
    shared = sorted(set(bm) & set(cm))
    same = 0
    for k in shared:
        if file_hash(bm[k]) == file_hash(cm[k]):
            same += 1
    bl, cl = loc(bin_path), loc(crate_path)
    if not only_bin and not only_crate and same == len(shared):
        return "identical", f"files={len(shared)}"
    if only_bin or bl > cl + 50:
        return "bin_ahead", f"only_bin={only_bin[:5]} loc {bl}>{cl}"
    if only_crate or cl > bl + 50:
        return "crate_ahead", f"only_crate={only_crate[:5]} loc {cl}>{bl}"
    if same == len(shared):
        return "identical_norm", "structure match soft"
    return "diverged", f"same={same}/{len(shared)} loc bin={bl} crate={cl}"


def analyze(onda_filter: int | None = None) -> list[dict]:
    rows = []
    for bin_rel, crate_root, crate_rel, onda, notes in PAIRS:
        if onda_filter is not None and onda != onda_filter:
            continue
        bin_path = BIN / bin_rel
        crate_path = ROOT / crate_root / crate_rel
        status, detail = classify(bin_path, crate_path)
        # Special: net is role_diff
        if bin_rel == "net.rs":
            status, detail = "role_diff", "bin=stack; k_nano=nic_globals"
        rows.append(
            {
                "module": bin_rel,
                "crate": f"{crate_root}/{crate_rel}",
                "onda": onda,
                "loc_bin": loc(bin_path) if bin_path.exists() else 0,
                "loc_crate": loc(crate_path) if crate_path.exists() else 0,
                "status": status,
                "detail": detail,
                "notes": notes,
                "cutover_ok": status
                in ("identical", "identical_norm", "stub", "crate_ahead"),
            }
        )
    return rows


def to_markdown(rows: list[dict]) -> str:
    lines = [
        "# BIN ↔ Crate Diff — Emagrecer neural-kernel (Onda 0)",
        "",
        "Gerado por `tools/diff_bin_crate.py`. Status:",
        "",
        "- `identical` / `identical_norm` — seguro `pub use`",
        "- `stub` — já cutover",
        "- `bin_ahead` — **promover bin→crate antes** de apagar",
        "- `crate_ahead` — absorver extras no bin, depois stub",
        "- `diverged` — diff manual",
        "- `role_diff` — não cutover cego",
        "",
        "| Módulo | Crate | Onda | LOC bin | LOC crate | Status | Cutover OK | Notas |",
        "|--------|-------|------|--------:|----------:|--------|:----------:|-------|",
    ]
    for r in rows:
        ok = "yes" if r["cutover_ok"] else "NO"
        notes = (r["notes"] + " · " + r["detail"]).strip(" ·")
        lines.append(
            f"| `{r['module']}` | `{r['crate']}` | {r['onda']} | {r['loc_bin']} | "
            f"{r['loc_crate']} | `{r['status']}` | {ok} | {notes} |"
        )
    lines.extend(
        [
            "",
            "## Gate checklist (por cutover)",
            "",
            "1. `python tools/diff_bin_crate.py --onda N` — sem `bin_ahead` nos alvos",
            "2. `cargo clean -p neural-kernel && cargo nk` = 0 erros",
            "3. Boot WHPX curto: 8 fases + `[TIMER] tick=`",
            "4. Se disco: ATA/FAT no serial; se net: não obrigatório ondas 1–3",
            "",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--onda", type=int, default=None, help="Filter by onda number")
    ap.add_argument(
        "--strict",
        action="store_true",
        help="Exit 1 if any non-stub target is bin_ahead/diverged/role_diff",
    )
    ap.add_argument("--markdown", type=Path, default=None, help="Write markdown table")
    ap.add_argument(
        "--allow-bin-ahead",
        action="store_true",
        help="With --strict, only fail on missing paths",
    )
    args = ap.parse_args()

    rows = analyze(args.onda)
    md = to_markdown(rows)
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(md, encoding="utf-8")
        print(f"Wrote {args.markdown}")
    else:
        print(md)

    if args.strict:
        bad = []
        for r in rows:
            if r["status"] == "stub":
                continue
            if r["status"] in ("missing_bin", "missing_crate", "missing"):
                bad.append(r)
            elif not args.allow_bin_ahead and r["status"] in (
                "bin_ahead",
                "diverged",
                "role_diff",
            ):
                bad.append(r)
        if bad:
            print("STRICT FAIL:", file=sys.stderr)
            for r in bad:
                print(f"  {r['module']}: {r['status']} — {r['detail']}", file=sys.stderr)
            return 1
        print("STRICT OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

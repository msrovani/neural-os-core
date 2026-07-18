#!/usr/bin/env python3
"""PreFlight Validator — neural-os-core ondas 0–7.

Uso:
  python tools/preflight_wave.py --wave 0
  python tools/preflight_wave.py --wave 1
  python tools/preflight_wave.py --idea 418
  python tools/preflight_wave.py --anti-fake-ready

Vereditos: SKIP | PARTIAL | DO | BLOCKED | AWAITING_HW
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOGS = ROOT / "logs"
IDEA = ROOT / "docs" / "memory" / "IDEA_BANK.md"
STATE = ROOT / "docs" / "memory" / "STATE.md"
INDEX = ROOT / "docs" / "architecture" / "INDEX.md"
SESSION_INDEX = ROOT / "docs" / "memory" / "SESSION_INDEX.md"
CACHE_DIR = ROOT / "docs" / "memory" / ".preflight_cache"

# Item -> (label, symbols or patterns in code, depends_on_lan, wave)
WAVE_ITEMS: dict[int, list[dict]] = {
    0: [
        {
            "id": "gaps",
            "label": "IDEA Gaps #277-283 / #416-423",
            "docs": ["Gaps amostrados", "#277", "depends_on: lan", "AWAITING_HW"],
            "code": [],
            "depends_on_lan": False,
        },
    ],
    1: [
        {
            "id": "422-evidence",
            "label": "NeuralFS evidência #422",
            "docs": ["power-loss", "smoke_multilevel", "NeuralFS"],
            "code": [
                "crates/k_nano/src/neural_fs/tests.rs",
                "smoke_power_loss",
                "smoke_level2",
            ],
            "depends_on_lan": False,
        },
    ],
    3: [
        {
            "id": "417",
            "label": "exFAT write #417",
            "docs": ["#417", "EXFAT_WRITE"],
            "code": [
                "crates/neural-kernel/src/exfat_write.rs",
                "smoke_write_roundtrip",
                "EXFAT_WRITE",
            ],
            "depends_on_lan": False,
        },
        {
            "id": "418",
            "label": "cloud sync #418",
            "docs": ["depends_on: lan", "#418"],
            "code": ["netfs"],
            "depends_on_lan": True,
            # Sempre fila Onda 7 — mesmo com RX>0 heurístico, cloud sync não é Onda 3.
            "force_blocked": "Onda 7 fila (depends_on: lan)",
        },
        {
            "id": "419",
            "label": "Storage UI #419",
            "docs": ["#419", "storage_report"],
            "code": ["storage_report", "storage_manager"],
            "depends_on_lan": False,
        },
    ],
    4: [
        {
            "id": "84",
            "label": "UAC isoc DMA #84",
            "docs": ["#84", "UAC-HW", "AWAITING_HW"],
            "code": [
                "crates/neural-kernel/src/audio/usb.rs",
                "UAC-HW",
                "AWAITING_REAL_HW",
            ],
            "depends_on_lan": False,
            "awaiting": ["UAC-HW", "awaiting_uac_isoc"],
        },
        {
            "id": "6",
            "label": "USB TrustTable #6",
            "docs": ["#6", "usb.tbl", "USB-TRUST"],
            "code": [
                "crates/neural-kernel/src/usb_trust.rs",
                "USB-TRUST",
                "usb.tbl",
            ],
            "depends_on_lan": False,
        },
        {
            "id": "12-15",
            "label": "USB policy #12-15",
            "docs": ["#12", "#15", "USB_TRUST_ENFORCE"],
            "code": [
                "decide",
                "disable_untrusted_ports",
                "USB_TRUST_ENFORCE",
            ],
            "depends_on_lan": False,
        },
        {
            "id": "soft-float",
            "label": "soft-float/VITS defer",
            "docs": ["soft-float", "VITS", "neural-lite"],
            "code": ["neural-lite", "soft-float"],
            "depends_on_lan": False,
            "force_blocked": "defer honesto — blocker soft-float (nao fakear VITS)",
        },
    ],
    5: [
        {
            "id": "420",
            "label": "MHI DMA #420",
            "docs": ["#420", "MHI-DMA", "AWAITING_HW"],
            "code": [
                "crates/k_nano/src/mhi.rs",
                "MHI-DMA",
                "register_vram_allocator",
            ],
            "depends_on_lan": False,
            "awaiting": ["MHI-DMA", "AWAITING_REAL_HW"],
        },
        {
            "id": "423",
            "label": "GPU Direct Storage #423",
            "docs": ["#423", "GDS-HW"],
            "code": [
                "crates/k_hal/src/gpu/direct_storage.rs",
                "probe_gds",
                "GDS-HW",
            ],
            "depends_on_lan": False,
            "awaiting": ["GDS-HW", "gpu_direct_storage"],
        },
        {
            "id": "454-456",
            "label": "GPU multigen #454-456",
            "docs": ["#454", "#455", "#456", "GPU-HW"],
            "code": [
                "GPU-HW",
                "log_gpu_hw_verdict",
                "canary",
            ],
            "depends_on_lan": False,
            "awaiting": ["GPU-HW", "AWAITING_REAL_HW"],
        },
        {
            "id": "67",
            "label": "AllocTier::Vram #67",
            "docs": ["#67", "VRAM", "register_vram_allocator"],
            "code": ["register_vram_allocator", "vram_alloc", "AllocTier::Vram"],
            "depends_on_lan": False,
        },
    ],
    6: [
        {
            "id": "airllm-ata",
            "label": "AirLLM ATA path",
            "docs": ["ADR-0046", "hot_swap_from_ata", "AIRLLM"],
            "code": [
                "crates/neural-kernel/src/gguf_streaming.rs",
                "hot_swap_from_ata",
                "PrefetchEngine",
            ],
            "depends_on_lan": False,
        },
        {
            "id": "airllm-dma",
            "label": "AirLLM DMA/stream/K-quant",
            "docs": ["AIRLLM-DMA", "stream-to-disk", "K-quant"],
            "code": [
                "AIRLLM-DMA",
                "stream_to_disk_deferred",
                "log_airllm_residuals",
            ],
            "depends_on_lan": False,
            "awaiting": ["AIRLLM-DMA", "AWAITING_REAL_HW"],
        },
        {
            "id": "airllm-net",
            "label": "AirLLM Net/L3.5 model-fetch",
            "docs": ["depends_on: lan", "L3.5", "model-fetch", "hot_swap_from_net"],
            "code": ["hot_swap_from_net", "http_get_range_host", "http_new_host_ranged"],
            "depends_on_lan": True,
            "pass_marker": "hot_swap Net",
        },
    ],
    7: [
        {
            "id": "lan",
            "label": "LAN RX/DHCP (crônico)",
            "docs": ["Onda 7", "NET-HW", "rx_count"],
            "code": ["NET-HW", "net_rx_count", "e1000", "virtio_net"],
            "depends_on_lan": False,
            "awaiting": ["lan_rx_zero_onda7", "AWAITING_REAL_HW reason=lan"],
            "pass_marker": "reason=rx_alive",
        },
        {
            "id": "418",
            "label": "cloud sync #418",
            "docs": ["#418", "NETFS", "netfs_peer"],
            "code": ["netfs", "smoke_if_online", "tcp_exchange"],
            "depends_on_lan": True,
            "pass_marker": "NETFS] [info] - VERDICT=PASS",
        },
        {
            "id": "tls-fetch",
            "label": "TLS/fetch/HTTP update",
            "docs": ["#124", "#308", "ADR-0016 N4"],
            "code": ["https_get", "tls_not_ready", "fetch_update"],
            "depends_on_lan": True,
            # pass_marker only when real HTTPS works — stub logs BLOCKED (PARTIAL ok)
            "pass_marker": "TLS] [info] - VERDICT=PASS",
        },
        {
            "id": "wifi",
            "label": "WiFi (crônico)",
            "docs": ["depends_on: wifi", "WIFI-HW", "WifiAgent"],
            "code": ["WifiAgent", "iwlwifi", "WIFI-HW"],
            "depends_on_lan": False,
            "awaiting": ["WIFI-HW", "AWAITING_REAL_HW"],
            "pass_marker": "WIFI-HW] [info] - VERDICT=PASS",
        },
    ],
}

IDEA_BY_ID: dict[str, dict] = {
    "417": {"wave": 3, "depends_on_lan": False, "label": "exFAT write"},
    "418": {"wave": 7, "depends_on_lan": True, "label": "cloud sync"},
    "419": {"wave": 3, "depends_on_lan": False, "label": "Storage UI"},
    "420": {"wave": 5, "depends_on_lan": False, "label": "MHI DMA"},
    "421": {"wave": 3, "depends_on_lan": False, "label": "SysInstaller"},
    "422": {"wave": 1, "depends_on_lan": False, "label": "NeuralFS disco"},
    "423": {"wave": 5, "depends_on_lan": False, "label": "GPU Direct"},
    "73": {"wave": 7, "depends_on_lan": False, "label": "VirtIO-net"},
    "124": {"wave": 7, "depends_on_lan": True, "label": "TLS 1.3"},
}


def read_text(path: Path) -> str:
    if not path.exists():
        return ""
    return path.read_text(encoding="utf-8", errors="replace")


def repo_has(pattern: str) -> bool:
    """Search pattern in docs + crates (bounded)."""
    hits = 0
    for base in (ROOT / "docs", ROOT / "crates", ROOT / "tools"):
        if not base.exists():
            continue
        for p in base.rglob("*"):
            if not p.is_file():
                continue
            if p.suffix.lower() not in {".md", ".rs", ".py", ".toml", ".txt"}:
                continue
            try:
                text = p.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            if pattern in text:
                hits += 1
                if hits >= 1:
                    return True
    return False


def latest_boot_logs(n: int = 8) -> list[Path]:
    if not LOGS.exists():
        return []
    files = sorted(LOGS.glob("boot_*.txt"), key=lambda p: p.stat().st_mtime, reverse=True)
    return files[:n]


def log_has(pattern: str) -> bool:
    for f in latest_boot_logs():
        try:
            if pattern in f.read_text(encoding="utf-8", errors="ignore"):
                return True
        except OSError:
            continue
    return False


def rx_ok() -> bool:
    for f in latest_boot_logs(12):
        try:
            text = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        # Heuristic: rx_count>0 or similar
        for m in re.finditer(r"rx_count\s*[=:]\s*(\d+)", text, re.I):
            if int(m.group(1)) > 0:
                return True
        if re.search(r"\[NET[^\]]*\][^\n]*rx[^\n]*[1-9]\d*", text, re.I):
            return True
    return False


def anti_fake_ready() -> list[str]:
    """Fail if VERDICT=PASS / has_compute Ready without boot_hw evidence."""
    issues: list[str] = []
    hw_logs = list(LOGS.glob("boot_hw_*.txt")) if LOGS.exists() else []
    boot_log_usb = False
    for f in latest_boot_logs(20):
        try:
            t = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        if "BOOT.LOG" in t and "VERDICT=PASS" in t:
            boot_log_usb = True
        if "VERDICT=PASS" in t and "GPU-HW" in t:
            if not hw_logs and not boot_log_usb:
                # PASS in qemu boot without boot_hw_* is suspicious unless AWAITING also present
                if "AWAITING_REAL_HW" not in t:
                    issues.append(f"{f.name}: VERDICT=PASS GPU-HW sem logs/boot_hw_*")
    # Docs claiming Ready
    state = read_text(STATE)
    if re.search(r"has_compute\s*=\s*true|Ready.*golden.*✅", state, re.I):
        if not hw_logs:
            issues.append("STATE.md sugere Ready/golden sem logs/boot_hw_*")
    return issues


def verdict_for_item(item: dict) -> tuple[str, str]:
    # force_blocked com depends_on_lan só vale enquanto RX=0; após lan_rx_ok some o bloqueio.
    if item.get("force_blocked"):
        if not (item.get("depends_on_lan") and rx_ok()):
            return "BLOCKED", str(item["force_blocked"])
    if item.get("depends_on_lan") and not rx_ok():
        return "BLOCKED", "depends_on: lan e rx_count não >0 nos logs recentes"

    docs_ok = all(
        (d in read_text(IDEA) or d in read_text(STATE) or d in read_text(INDEX) or repo_has(d))
        for d in item.get("docs", [])
    ) if item.get("docs") else True

    code_patterns = item.get("code", [])
    code_hits = sum(1 for c in code_patterns if repo_has(c) or (ROOT / c).exists())
    code_ok = code_hits >= max(1, len(code_patterns) // 2) if code_patterns else True

    # pass_marker específico do domínio (evita NET-HW PASS contaminar WiFi/UAC/GPU).
    pass_marker = item.get("pass_marker")
    passed = bool(pass_marker) and log_has(str(pass_marker))

    # AWAITING só com marcadores próprios; se já há PASS do domínio, não AWAITING.
    awaiting_markers = item.get("awaiting", [])
    awaiting = False
    if awaiting_markers and not passed:
        st = read_text(STATE)
        awaiting = any(log_has(m) or m in st for m in awaiting_markers)

    if passed and code_ok and docs_ok:
        return "SKIP", "código+docs+PASS (marker)"
    if awaiting and code_ok:
        return "AWAITING_HW", "path/log AWAITING — não reimplementar; coletar log HW"
    if code_ok and not docs_ok:
        return "PARTIAL", "código existe; sync docs"
    if docs_ok and not code_ok:
        return "DO", "docs/tag ok; falta código/símbolo"
    if code_ok and docs_ok:
        return "PARTIAL", "presente; falta evidência runtime/aceite"
    return "DO", "ausente ou incompleto"


def write_session_cache(wave: int, rows: list[tuple[str, str, str]]) -> Path:
    """Cache PreFlight por onda (consulta em sessões seguintes)."""
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    path = CACHE_DIR / f"wave_{wave}.txt"
    lines = [
        f"# PreFlight cache wave={wave}",
        f"lan_rx_ok={rx_ok()}",
        f"logs_scanned={len(latest_boot_logs())}",
    ]
    for iid, v, why in rows:
        lines.append(f"{iid}\t{v}\t{why}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def run_wave(wave: int) -> int:
    items = WAVE_ITEMS.get(wave)
    if items is None:
        print(f"[PreFlight] wave={wave} sem checklist embutido (OK se só docs)")
        return 0
    print(f"=== PreFlight wave {wave} ===")
    print(f"lan_rx_ok={rx_ok()} logs_scanned={len(latest_boot_logs())}")
    worst = 0
    rows: list[tuple[str, str, str]] = []
    for item in items:
        v, why = verdict_for_item(item)
        flag = {"SKIP": 0, "PARTIAL": 1, "AWAITING_HW": 1, "BLOCKED": 2, "DO": 3}.get(v, 3)
        # force_blocked = fila consciente (ex. #418 Onda 7) — nao falha a onda atual
        if v == "BLOCKED" and item.get("force_blocked"):
            flag = 1
        # depends_on_lan BLOCKED na onda 6/7 checklist = esperado
        if v == "BLOCKED" and item.get("depends_on_lan") and wave in (6, 7):
            flag = 1
        worst = max(worst, flag)
        print(f"  [{v:12}] {item['id']:16} {item['label']} — {why}")
        rows.append((str(item["id"]), v, why))
    cache = write_session_cache(wave, rows)
    print(f"cache={cache.relative_to(ROOT)}")
    return 0 if worst <= 1 else 1


def run_idea(idea_id: str) -> int:
    meta = IDEA_BY_ID.get(idea_id)
    if not meta:
        print(f"[PreFlight] idea #{idea_id} não está no mapa embutido")
        return 1
    item = {
        "id": idea_id,
        "label": meta["label"],
        "docs": [f"#{idea_id}", "depends_on: lan" if meta["depends_on_lan"] else idea_id],
        "code": [],
        "depends_on_lan": meta["depends_on_lan"],
    }
    v, why = verdict_for_item(item)
    print(f"=== PreFlight idea #{idea_id} wave={meta['wave']} ===")
    print(f"  [{v}] {meta['label']} — {why}")
    if meta["depends_on_lan"]:
        print(f"  tag: depends_on: lan | lan_rx_ok={rx_ok()}")
    return 0 if v in ("SKIP", "PARTIAL", "AWAITING_HW", "BLOCKED") else 1


def main() -> int:
    ap = argparse.ArgumentParser(description="PreFlight Validator ondas neural-os-core")
    ap.add_argument("--wave", type=int, help="Número da onda (0–7)")
    ap.add_argument("--idea", type=str, help="ID IDEA (ex: 418)")
    ap.add_argument("--anti-fake-ready", action="store_true", help="Checa Ready/PASS sem boot_hw")
    args = ap.parse_args()

    if args.anti_fake_ready:
        issues = anti_fake_ready()
        if issues:
            print("[anti-fake-ready] FAIL")
            for i in issues:
                print(f"  - {i}")
            return 2
        print("[anti-fake-ready] OK")
        return 0

    if args.idea:
        return run_idea(args.idea.lstrip("#"))
    if args.wave is not None:
        return run_wave(args.wave)

    ap.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""llm_ladder_bench.py — compara BitNet 850 / 1.3 / 2B / 3B no QEMU.

Metricas: tamanho blob, tempo load FAT, tempo inferencia (ticks/~s),
hub_status, texto Generated, score coerencia heuristico.

Importante: NAO anexa QEMU-loader do 2B (senao Active vira sempre 2B).
Uso:
  python tools/llm_ladder_bench.py --steps 850,13
  python tools/llm_ladder_bench.py --steps 850,13,2b,3b --accel whpx
"""
from __future__ import annotations

import argparse
import json
import os
import re
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
QEMU = Path(r"C:\Program Files\qemu\qemu-system-x86_64.exe")
TARGET = ROOT / "target"
LOG_DIR = ROOT / "logs" / "llm_ladder"
PROMPTS = [
    "ola",
    "quanto e 2 mais 2",
    "o que e neural os",
]
LLM_TEST_RE = re.compile(
    r"LLM-TEST.*?prompt='([^']*)'.*?ticks=(\d+).*?\(~(\d+)s\).*?response='([^']*)'",
    re.S,
)
# Fase 0: parse token-level gen info
GEN_STEP_RE = re.compile(r"\[GEN\].*?step=(\d+)\s+next=(\d+)\s+cols=(\d+)")
GEN_EOS_RE = re.compile(r"\[GEN\].*?eos/special at step=(\d+)\s+id=(\d+)")
GEN_KV_RE = re.compile(r"\[GEN\].*?step=(\d+)\s+token=(\d+)\s+kv_cache:\s*(\d+)\s+ticks")
FWD_LOGITS_RE = re.compile(
    r"\[FWD\].*?logits_top_n=(\d+)\s+ids=\[([^\]]+)\]\s+logits_bits=\[([^\]]+)\]"
)
EARLY_EXIT_RE = re.compile(r"\[GEN\].*?early_exit\s+(\S+)\s+step=(\d+)")

# QEMU sendkey names (PS/2 set1 / QEMU key names)
CHAR_KEY = {
    " ": "spc",
    "-": "minus",
    "=": "equal",
    ".": "dot",
    ",": "comma",
    "/": "slash",
    "'": "apostrophe",
    ";": "semicolon",
}


def pack_disk(step: str, size_mb: int) -> None:
    """FAT32 obrigatorio: boot Active LLM so le type 0x0B/0x0C (exFAT 0x07 = ABSENT)."""
    env = os.environ.copy()
    env["PACK_LLM"] = step
    env["BOOT_MODE"] = "qemu"
    cmd = [
        sys.executable,
        str(ROOT / "tools" / "build_image.py"),
        "--fat32",
        "--size",
        str(size_mb),
        "--output",
        str(TARGET / "disk_qemu.raw"),
    ]
    print(f"[PACK] PACK_LLM={step} size={size_mb} fs=FAT32")
    subprocess.check_call(cmd, cwd=str(ROOT), env=env)


def blob_info(step: str) -> dict:
    mapping = {
        "850": ("BITNET850.BIN", "bitnet_850m.bitnet"),
        "13": ("BITNET13.BIN", "bitnet_1p3b.bitnet"),
        "2b": ("BITNET2B.BIN", "bitnet_2B.bitnet"),
        "3b": ("BITNET3B.BIN", "bitnet_3B.bitnet"),
    }
    names = mapping[step]
    for n in names:
        p = TARGET / n
        if p.exists() and p.stat().st_size > 1_000_000:
            return {"file": n, "bytes": p.stat().st_size, "mb": round(p.stat().st_size / 1e6, 1)}
    return {"file": None, "bytes": 0, "mb": 0}


def qemu_args(
    logfile: Path,
    mon_port: int,
    ram_gb: int,
    smp: int,
    accel: str,
    *,
    model_loader: Path | None,
    step: str = "850",
) -> list[str]:
    cpu = "Haswell" if accel == "whpx" else "max"
    args = [
        str(QEMU),
        "-m", f"{ram_gb}G",
        "-smp", str(smp),
        "-accel", accel,
        "-cpu", cpu,
        "-drive", f"format=raw,file={TARGET / 'uefi.img'},if=ide,index=0",
        "-drive", f"format=raw,file={TARGET / 'disk_qemu.raw'},if=ide,index=1",
        "-drive", f"if=pflash,format=raw,file={TARGET / 'ovmf.fd'},readonly=on",
        "-serial", f"file:{logfile}",
        "-serial", "null",
        "-netdev", "user,id=n0",
        "-device", "e1000,netdev=n0",
        "-audiodev", "none,id=snd0",
        "-device", "intel-hda,id=hda0",
        "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
        "-device", "qemu-xhci,id=xhci",
        "-device", "usb-tablet,bus=xhci.0",
        "-device", "usb-kbd,bus=xhci.0",
        "-device", "virtio-gpu-pci,id=vgpu",
        "-vga", "std",
        "-display", "none",
        "-monitor", f"tcp:127.0.0.1:{mon_port},server,nowait",
    ]
    # Modelo sob teste via QEMU-loader @4GB (mesmo path do 2B; magica BE11).
    # Evita PIO ATA de 180–800MB (horas) e isola inferencia/coerencia.
    if model_loader is not None and model_loader.exists():
        args += ["-device", f"loader,file={model_loader},addr=0x100000000"]
        print(f"[LOADER] {model_loader.name} @0x100000000 ({model_loader.stat().st_size/1e6:.0f}MB)")
    # BPE: 850/13/3b = SentencePiece 32k; 2b = Llama-3 128k
    bpe_path = TARGET / "bpe_vocab.bin"
    if step in ("850", "13", "3b"):
        sp32 = TARGET / "bpe_vocab_sp32.bin"
        if sp32.exists():
            bpe_path = sp32
        else:
            print(f"[WARN] {sp32.name} ausente — usando {bpe_path.name} (pode quebrar coerencia 32k)")
    for path, addr in [
        (TARGET / "PIPER_PT_BR.BIN", "0x130000000"),
        (bpe_path, "0x150000000"),
        (TARGET / "hw_expert_v3.bitnet", "0x160000000"),
        (TARGET / "rust_coder.bitnet", "0x161000000"),
        (TARGET / "bge-small.bitnet", "0x162000000"),
        (TARGET / "STT.BIN", "0x163000000"),
    ]:
        if path.exists():
            args += ["-device", f"loader,file={path},addr={addr}"]
            if path == bpe_path:
                print(f"[LOADER] {path.name} @0x150000000 ({path.stat().st_size/1024:.0f}KB)")
    return args


def mon_connect(port: int, timeout: float = 60.0) -> socket.socket:
    t0 = time.time()
    last = None
    while time.time() - t0 < timeout:
        try:
            s = socket.create_connection(("127.0.0.1", port), timeout=2)
            s.settimeout(5)
            # drain banner
            try:
                s.recv(4096)
            except socket.timeout:
                pass
            return s
        except OSError as e:
            last = e
            time.sleep(0.4)
    raise RuntimeError(f"monitor connect failed: {last}")


def mon_cmd(s: socket.socket, cmd: str) -> None:
    s.sendall((cmd + "\n").encode("ascii", errors="ignore"))
    time.sleep(0.05)
    try:
        s.recv(8192)
    except socket.timeout:
        pass


def send_text(s: socket.socket, text: str) -> None:
    for ch in text.lower():
        if "a" <= ch <= "z" or "0" <= ch <= "9":
            key = ch
        elif ch in CHAR_KEY:
            key = CHAR_KEY[ch]
        else:
            continue
        mon_cmd(s, f"sendkey {key}")
        time.sleep(0.03)
    mon_cmd(s, "sendkey ret")


def tail_contains(path: Path, patterns: list[str], since_pos: int) -> tuple[bool, int, str]:
    if not path.exists():
        return False, since_pos, ""
    data = path.read_bytes()
    if len(data) <= since_pos:
        return False, since_pos, ""
    chunk = data[since_pos:]
    text = chunk.decode("utf-8", errors="replace")
    for p in patterns:
        if p in text:
            return True, len(data), text
    return False, since_pos, text


def wait_log(path: Path, patterns: list[str], timeout: float, pos: int = 0) -> tuple[bool, int, str]:
    t0 = time.time()
    last = ""
    while time.time() - t0 < timeout:
        ok, pos, chunk = tail_contains(path, patterns, pos)
        if chunk:
            last = chunk
        if ok:
            return True, pos, last
        time.sleep(0.5)
    return False, pos, last


def coherence_score(prompt: str, reply: str) -> dict:
    r = (reply or "").strip()
    low = r.lower()
    reasons = []
    score = 0
    if not r or r.startswith("(sem LLM"):
        return {"score": 0, "reasons": ["empty_or_no_model"]}
    if len(r) >= 8:
        score += 2
        reasons.append("len_ok")
    else:
        reasons.append("too_short")
    # repeticao / lixo
    if len(set(r)) < max(3, len(r) // 8):
        score -= 2
        reasons.append("low_charset")
    if re.search(r"(.)\1{8,}", r):
        score -= 2
        reasons.append("char_run")
    # PT/EN tokens basicos
    if any(w in low for w in ("ola", "olá", "oi", "hello", "hi", "bom", "sou", "i am", "jarbas", "neural")):
        score += 2
        reasons.append("greet_or_id")
    if "2" in prompt and any(x in low for x in ("4", "quatro", "four")):
        score += 3
        reasons.append("math_2p2")
    elif "2" in prompt:
        reasons.append("math_miss")
    if "neural" in prompt and any(x in low for x in ("os", "sistema", "operating", "ai", "kernel", "agent")):
        score += 2
        reasons.append("topic_hit")
    # garbage tokens típicos de modelo nao converteu bem
    if re.search(r"[^\x20-\x7e\u00c0-\u017f]{4,}", r):
        score -= 1
        reasons.append("binary_ish")
    return {"score": max(0, score), "reasons": reasons}


def parse_gen_steps(log_text: str) -> list[dict]:
    """Extrai steps individuais de geracao do log serial."""
    steps: list[dict] = []
    for m in GEN_STEP_RE.finditer(log_text):
        steps.append({
            "step": int(m.group(1)),
            "next_id": int(m.group(2)),
            "cols": int(m.group(3)),
        })
    # marcar stop reason e kv ticks
    step_map = {s["step"]: s for s in steps}
    for m in GEN_EOS_RE.finditer(log_text):
        sid = int(m.group(1))
        if sid in step_map:
            step_map[sid]["stop"] = "eos"
    for m in EARLY_EXIT_RE.finditer(log_text):
        sid = int(m.group(2))
        if sid in step_map:
            step_map[sid]["stop"] = f"early_{m.group(1)}"
    for m in GEN_KV_RE.finditer(log_text):
        sid = int(m.group(1))
        if sid in step_map:
            step_map[sid]["ticks_kv"] = int(m.group(3))
    # se o ultimo step nao tem stop, e max_gen
    if steps:
        if "stop" not in steps[-1]:
            steps[-1]["stop"] = "max_gen"
    return steps


def parse_gen_detail(log_text: str) -> dict | None:
    """Extrai token_ids, stop reason, bpe type e logits_top do log."""
    out: dict = {}
    # BPE type: kernel log "BPB1 LOADED ... sp32=true/false"
    bpe_sp32 = re.search(r"BPB1 LOADED.*sp32=(true|false)", log_text)
    if bpe_sp32:
        out["bpe"] = "sp32" if bpe_sp32.group(1) == "true" else "llama"
    elif "BPB1" in log_text:
        out["bpe"] = "sp32" if "sp32" in log_text.lower() else "llama"
    if re.search(r"BPB1 LOADED.*vocab_n=(\d+)", log_text):
        out["vocab_n"] = int(re.search(r"vocab_n=(\d+)", log_text).group(1))
    # token ids do gen
    steps = parse_gen_steps(log_text)
    if steps:
        out["token_ids"] = [s["next_id"] for s in steps]
        stop_reasons = [s.get("stop") for s in steps if s.get("stop")]
        if stop_reasons:
            out["stop_reason"] = stop_reasons[0]
        else:
            out["stop_reason"] = "max_gen"
        if any(s.get("ticks_kv") for s in steps):
            out["kv_ticks_sum"] = sum(s.get("ticks_kv", 0) for s in steps)
    # logits top do FWD (primeiro dump apenas)
    fwd = FWD_LOGITS_RE.search(log_text)
    if fwd:
        out["logits_top_n"] = int(fwd.group(1))
        out["logits_ids"] = [int(x.strip()) for x in fwd.group(2).split(",") if x.strip()]
    out["gen_steps"] = steps
    return out or None


def parse_metrics(log_text: str, prompt: str) -> dict:
    loaded = re.findall(r"LLM LOADED file=(\S+) size=(\d+)KB", log_text)
    hub = re.findall(r"ModelHub:[^\n]+", log_text)
    gens = re.findall(
        r'Generating for: "([^"]*)"[^\n]*\n.*?generate_via_model took (\d+) ticks \(~(\d+)s\).*?\n.*?Generated: "([^"]*)"',
        log_text,
        re.S,
    )
    # fallback Generated lines
    if not gens:
        gens_simple = re.findall(r'Generated: "([^"]*)"', log_text)
        ticks = re.findall(r"generate_via_model took (\d+) ticks \(~(\d+)s\)", log_text)
        gens = []
        for i, g in enumerate(gens_simple):
            t = ticks[i] if i < len(ticks) else ("?", "?")
            gens.append((prompt, t[0], t[1], g))
    # pick last matching prompt if possible
    pick = None
    for g in gens:
        if len(g) == 4 and (prompt in g[0] or g[0] == prompt or not pick):
            pick = g
    if gens and not pick:
        pick = gens[-1]
    out = {
        "loaded": loaded[-1] if loaded else None,
        "hub": hub[-1] if hub else None,
        "gen": None,
    }
    if pick:
        out["gen"] = {
            "for": pick[0],
            "ticks": int(pick[1]) if str(pick[1]).isdigit() else None,
            "secs_approx": int(pick[2]) if str(pick[2]).isdigit() else None,
            "text": pick[3],
        }
    return out


def run_step(
    step: str,
    *,
    size_mb: int,
    ram_gb: int,
    smp: int,
    accel: str,
    mon_port: int,
    boot_timeout: float,
    infer_timeout: float,
    pack: bool,
    via: str,
) -> dict:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    info = blob_info(step)
    if info["bytes"] == 0:
        return {"step": step, "error": "blob_missing", "blob": info}

    if pack:
        pack_disk(step, size_mb)

    logfile = LOG_DIR / f"boot_{step}_{int(time.time())}.txt"
    if logfile.exists():
        logfile.unlink()
    model_path = TARGET / info["file"] if info["file"] else None
    use_loader = via == "loader"
    args = qemu_args(
        logfile,
        mon_port,
        ram_gb,
        smp,
        accel,
        model_loader=model_path if use_loader else None,
        step=step,
    )
    print(f"[QEMU] step={step} via={via} accel={accel} log={logfile.name} blob={info}")
    proc = subprocess.Popen(args, cwd=str(ROOT))
    result = {
        "step": step,
        "blob": info,
        "logfile": str(logfile),
        "accel": accel,
        "via": via,
        "load": {},
        "prompts": [],
        "error": None,
    }
    try:
        t_boot = time.time()
        mon = mon_connect(mon_port, timeout=90)
        # Esperar so sinais de LLM (AgentFleet vem antes do load e enganava o bench)
        ok, pos, chunk = wait_log(
            logfile,
            [
                "LLM LOADED file=",
                "LLM ABSENT",
                "no model — ABSENT",
                "BOOT: LLM ABSENT",
            ],
            boot_timeout,
            0,
        )
        load_s = round(time.time() - t_boot, 1)
        full = logfile.read_text(encoding="utf-8", errors="replace") if logfile.exists() else ""
        m = parse_metrics(full, "")
        loaded_ok = ("LLM LOADED" in full) and ("LLM ABSENT" not in full or "LLM LOADED" in full)
        # Preferencia: se tem LOADED, ok mesmo com ABSENT depois (nao deve)
        loaded_ok = "LLM LOADED" in full
        result["load"] = {
            "ok": loaded_ok,
            "seconds": load_s,
            "loaded": m["loaded"],
            "hub": m["hub"],
            "boot_ready": ok,
            "via": via,
        }
        if not result["load"]["ok"]:
            result["error"] = "no_llm_loaded"
            mon_cmd(mon, "quit")
            return result

        # LLM-TEST no boot roda os 3 prompts (sendkey e flaky em -display none)
        print("  [WAIT] LLM-TEST boot prompts...")
        ok_test, _, _ = wait_log(
            logfile,
            ["LLM-TEST", "#3/", "response='", "[EXC] #PF"],
            max(infer_timeout * 3, 1200),
            pos,
        )
        full = logfile.read_text(encoding="utf-8", errors="replace")
        if "[EXC] #PF" in full and "LLM-TEST" not in full:
            result["error"] = "page_fault_during_infer"
            mon_cmd(mon, "quit")
            return result

        found = list(LLM_TEST_RE.finditer(full))
        # slog pode quebrar a linha; fallback linha-a-linha
        if not found:
            for line in full.splitlines():
                if "LLM-TEST" not in line or "prompt=" not in line:
                    continue
                m2 = re.search(
                    r"prompt='([^']*)'.*?ticks=(\d+).*?\(~(\d+)s\).*?response='([^']*)'",
                    line,
                )
                if m2:
                    found.append(m2)

        # Fase 0: gen detail global (primeiro prompt)
        gen_detail = parse_gen_detail(full)
        if gen_detail:
            result["gen_detail"] = gen_detail

        if found:
            for m2 in found:
                prompt, ticks, secs, text = m2.group(1), m2.group(2), m2.group(3), m2.group(4)
                coh = coherence_score(prompt, text)
                entry = {
                    "prompt": prompt,
                    "wait_s": int(secs),
                    "got_response": True,
                    "ticks": int(ticks),
                    "secs_approx": int(secs),
                    "reply": text,
                    "coherence": coh,
                    "source": "llm-test-boot",
                }
                # Fase 0: attach gen detail per-prompt (token_ids, stop_reason)
                if gen_detail:
                    entry["token_ids"] = gen_detail.get("token_ids", [])
                    entry["stop_reason"] = gen_detail.get("stop_reason")
                    entry["logits_top_ids"] = gen_detail.get("logits_ids", [])
                result["prompts"].append(entry)
                print(
                    f"  [ASK] {prompt!r} -> {text!r} ticks={ticks} ~{secs}s "
                    f"coh={coh['score']} {coh['reasons']}"
                )
            if not ok_test and not result["prompts"]:
                result["error"] = "llm_test_timeout"
        else:
            result["error"] = result.get("error") or "no_llm_test_lines"
            print("  [WARN] sem LLM-TEST no log — teclado fallback omitido (flaky)")

        mon_cmd(mon, "quit")
        try:
            mon.close()
        except Exception:
            pass
    except Exception as e:
        result["error"] = str(e)
        print(f"[ERR] {e}")
        try:
            proc.terminate()
        except Exception:
            pass
    finally:
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.kill()
    return result


def summarize(results: list[dict]) -> str:
    lines = [
        "# LLM ladder — viabilidade",
        "",
        "| Degrau | Blob MB | Load s | Load file | Infer ~s (med) | Coh med | BPE | Tokens | Stop | Notas |",
        "|--------|---------|--------|-----------|----------------|---------|-----|--------|------|-------|",
    ]
    for r in results:
        blob = r.get("blob", {}).get("mb", "?")
        load = r.get("load", {})
        ps = r.get("prompts") or []
        secs = [p.get("secs_approx") for p in ps if p.get("secs_approx") is not None]
        cohs = [p.get("coherence", {}).get("score", 0) for p in ps]
        med_s = round(sum(secs) / len(secs), 1) if secs else "-"
        med_c = round(sum(cohs) / len(cohs), 1) if cohs else 0
        note = r.get("error") or (load.get("loaded") and str(load.get("loaded"))) or ""
        # Fase 0: gen detail
        gd = r.get("gen_detail") or {}
        bpe = gd.get("bpe", "")
        token_ids = gd.get("token_ids", [])
        n_tok = len(token_ids)
        stop = gd.get("stop_reason", "")
        tok_str = ",".join(str(t) for t in token_ids[:8])
        if len(token_ids) > 8:
            tok_str += "…"
        lines.append(
            f"| {r.get('step')} | {blob} | {load.get('seconds', '-')} | "
            f"{load.get('loaded')} | {med_s} | {med_c} | {bpe} | {tok_str} | {stop} | {note} |"
        )
    lines.append("")
    lines.append("## Respostas")
    for r in results:
        lines.append(f"### {r.get('step')}")
        for p in r.get("prompts") or []:
            extra = ""
            if p.get("token_ids"):
                extra += f" ids={p['token_ids']}"
            if p.get("stop_reason"):
                extra += f" stop={p['stop_reason']}"
            if p.get("logits_top_ids"):
                extra += f" top5={p['logits_top_ids'][:5]}"
            lines.append(f"- Q: `{p['prompt']}` → `{p.get('reply','')}` (coh={p.get('coherence',{}).get('score')}){extra}")
        lines.append("")
    # veredicto simples
    viable = []
    for r in results:
        if r.get("error"):
            continue
        ps = r.get("prompts") or []
        if not ps:
            continue
        cohs = [p.get("coherence", {}).get("score", 0) for p in ps]
        secs = [p.get("secs_approx") or p.get("wait_s") or 999 for p in ps]
        avg_c = sum(cohs) / len(cohs)
        avg_s = sum(secs) / len(secs)
        mb = r.get("blob", {}).get("mb") or 0
        # custo relativo ~ mb * avg_s / max(coh,0.1)
        cost = (mb * avg_s) / max(avg_c, 0.5)
        viable.append((r["step"], avg_c, avg_s, mb, cost))
    if viable:
        viable.sort(key=lambda x: (-x[1], x[4], x[2]))
        lines.append("## Veredicto (heuristico)")
        lines.append(
            f"Melhor equilibrio coerencia/custo: **{viable[0][0]}** "
            f"(coh={viable[0][1]:.1f}, infer~{viable[0][2]:.0f}s, {viable[0][3]}MB, cost={viable[0][4]:.0f})"
        )
        for v in viable:
            lines.append(f"- {v[0]}: coh={v[1]:.1f} infer~{v[2]:.0f}s mem={v[3]}MB cost_idx={v[4]:.0f}")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", default="850,13,2b,3b")
    ap.add_argument("--size", type=int, default=3072)
    ap.add_argument("--ram", type=int, default=6)
    ap.add_argument("--smp", type=int, default=2)
    ap.add_argument("--accel", default="whpx", choices=["whpx", "tcg"])
    ap.add_argument("--boot-timeout", type=float, default=900)
    ap.add_argument("--infer-timeout", type=float, default=600)
    ap.add_argument("--no-pack", action="store_true")
    ap.add_argument(
        "--via",
        default="loader",
        choices=["loader", "fat"],
        help="loader=QEMU phys map (rapido, isola inferencia); fat=PIO ATA FAT32 (realista/lento)",
    )
    ap.add_argument("--mon-port", type=int, default=4555)
    args = ap.parse_args()

    if not QEMU.exists() or not (TARGET / "uefi.img").exists():
        print("[FATAL] QEMU ou uefi.img ausente")
        return 2

    steps = [s.strip().lower() for s in args.steps.split(",") if s.strip()]
    results = []
    for i, step in enumerate(steps):
        if step not in ("850", "13", "2b", "3b"):
            print(f"[SKIP] step invalido {step}")
            continue
        r = run_step(
            step,
            size_mb=args.size,
            ram_gb=args.ram,
            smp=args.smp,
            accel=args.accel,
            mon_port=args.mon_port + i,
            boot_timeout=args.boot_timeout,
            infer_timeout=args.infer_timeout,
            pack=not args.no_pack,
            via=args.via,
        )
        results.append(r)
        out_json = LOG_DIR / f"result_{step}.json"
        out_json.write_text(json.dumps(r, indent=2, ensure_ascii=False), encoding="utf-8")

    report = summarize(results)
    report_path = LOG_DIR / "COMPARE.md"
    report_path.write_text(report, encoding="utf-8")
    (LOG_DIR / "results.json").write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
    print("\n" + report)
    print(f"\n[OK] report -> {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

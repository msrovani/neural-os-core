#!/usr/bin/env python3
"""Monitor de 30 min das 2 instancias do mesh (A/B).

Le SOMENTE: nao toca no QEMU, no log nem na rede. Amostra a cada 30 s:
bytes e tick de cada log, fase mais recente, marcadores de problema e RAM do host.
No fim, resume os problemas por contagem (nao por sensacao).

Marcadores: o runtime de falha do kernel e #PF / PF storm / SILENCE / OOM /
[SILENCE] / park core / Triple fault; o resto vem do slog [err]/[fail]/[warn].
Para o mesh: peers=, MESH_HEALTH, SKILL, CRDT.
"""
import io, os, re, sys, time, csv, subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGS = {"A": os.path.join(ROOT, "logs", "boot_mesh_a.txt"),
        "B": os.path.join(ROOT, "logs", "boot_mesh_b.txt")}
OUT_CSV = os.path.join(ROOT, "logs", "mesh_watch.csv")
STATUS = os.path.join(ROOT, "logs", "mesh_watch_status.txt")
SUMMARY = os.path.join(ROOT, "logs", "mesh_watch_summary.txt")

MINUTES = int(sys.argv[1]) if len(sys.argv) > 1 else 30
INTERVAL = 30

ANSI = re.compile(rb"\x1b\[[0-9;?]*[ -/]*[@-~]")
PROBLEMS = {
    "pf": rb"\[PF_DBG\]|#PF storm|page fault",
    "panic": rb"PANIC|panic=|Triple fault|KERNEL_ERROR",
    "silence": rb"\[SILENCE\]|OOM-HALT|park core",
    "oom": rb"OOM|out of memory|alloc fail|headroom",
    "exc": rb"\[EXC\]",
    "err": rb"\[err\]",
    "fail": rb"\[fail\]",
    "degraded": rb"DEGRADED",
    "timeout": rb"TIMEOUT",
    "mesh_peers": rb"peers=(\d+)",
}


def read(path):
    try:
        with open(path, "rb") as fh:
            fh.seek(0, os.SEEK_END)
            size = fh.tell()
            fh.seek(0)
            data = fh.read()
        return size, ANSI.sub(b"", data)
    except FileNotFoundError:
        return 0, b""


def host_mem():
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command",
                              "$os=Get-CimInstance Win32_OperatingSystem;"
                              "[math]::Round($os.FreePhysicalMemory/1MB,2)"],
                             capture_output=True, text=True, timeout=20)
        # PowerShell formata com virgula decimal Depending do locale: "8,02".
        # No CSV isso vira string ambigua -> normaliza para ponto.
        return out.stdout.strip().replace(",", ".")
    except Exception as e:
        return "n/a"


def qemu_count():
    """Quantas instancias do mesh ainda existem.

    Risco operacional conhecido: outra thread roda Stop-Process em todo
    qemu-system-x86_64. Sem esta coluna um log estagnado e indistinguivel de
    "QEMU morto" -- e o sintoma se disfarca de stall do guest.
    """
    try:
        out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq qemu-system-x86_64.exe"],
                             capture_output=True, text=True, timeout=20)
        return str(len(re.findall(r"qemu-system-x86_64\.exe", out.stdout)))
    except Exception:
        return "n/a"


def sample():
    row = {"ts": time.strftime("%H:%M:%S"), "free_gb": host_mem(), "qemu_n": qemu_count()}
    for tag, path in LOGS.items():
        size, data = read(path)
        ticks = [int(m) for m in re.findall(rb"\[T\+(\d+)\]", data)]
        phases = re.findall(rb"PHASE n=(\d) name=(\w+)", data)
        row[tag + "_bytes"] = size
        row[tag + "_tick"] = max(ticks) if ticks else 0
        row[tag + "_phase"] = phases[-1][1].decode() if phases else "-"
        for k, pat in PROBLEMS.items():
            if k == "mesh_peers":
                continue
            row[tag + "_" + k] = len(re.findall(pat, data, re.IGNORECASE))
        # Prioritiza o peers do mesh P2P/MESH_HEALTH sobre o do canal FL
        # (federated learning), que conta peers de forma diferente e emite
        # "crdt v=... peers=0" — falso-positivo que contaminava o CSV/summary.
        # (IDEA #656, S455: o canal [nk][FL] emite peers=0 que o [-1] pega.)
        mch = re.findall(rb"MESH_HEALTH.*?peers=(\d+)", data)
        if mch:
            row[tag + "_peers"] = mch[-1].decode()
        else:
            peers = re.findall(rb"peers=(\d+)", data)
            row[tag + "_peers"] = peers[-1].decode() if peers else "-"
    return row


FIELDS = ["ts", "free_gb", "qemu_n"] + [t + "_" + k for t in ("A", "B")
                              for k in ("bytes", "tick", "phase", "peers", "pf", "panic",
                                        "silence", "oom", "exc", "err", "fail", "degraded", "timeout")]
new = not os.path.exists(OUT_CSV)
with open(OUT_CSV, "a", newline="", encoding="ascii") as fh:
    w = csv.DictWriter(fh, fieldnames=FIELDS)
    if new:
        w.writeheader()
    end = time.time() + MINUTES * 60
    first = None
    while time.time() < end:
        r = sample()
        if first is None:
            first = r
        w.writerow({k: r.get(k, "") for k in FIELDS})
        fh.flush()
        with open(STATUS, "w", encoding="ascii") as st:
            st.write("t=%s free_gb=%s qemu_n=%s | A: %s B tick=%s phase=%s peers=%s pf=%s exc=%s err=%s fail=%s"
                     " | B: %s B tick=%s phase=%s peers=%s pf=%s exc=%s err=%s fail=%s\n" % (
                         r["ts"], r["free_gb"], r["qemu_n"],
                         r["A_bytes"], r["A_tick"], r["A_phase"], r["A_peers"], r["A_pf"], r["A_exc"], r["A_err"], r["A_fail"],
                         r["B_bytes"], r["B_tick"], r["B_phase"], r["B_peers"], r["B_pf"], r["B_exc"], r["B_err"], r["B_fail"]))
        time.sleep(INTERVAL)

# resumo final: deltas da janela, nao acumulado desde o boot
last = sample()
with open(SUMMARY, "w", encoding="ascii") as sf:
    sf.write("=== MESH 2 instancias - janela de %d min ===\n" % MINUTES)
    for tag in ("A", "B"):
        sf.write("\n[%s] log=%s\n" % (tag, LOGS[tag]))
        sf.write("  bytes=%d tick=%s fase=%s peers=%s\n" % (
            last[tag + "_bytes"], last[tag + "_tick"], last[tag + "_phase"], last[tag + "_peers"]))
        for k in ("pf", "panic", "silence", "oom", "exc", "err", "fail", "degraded", "timeout"):
            sf.write("  %-9s = %s\n" % (k, last[tag + "_" + k]))
    sf.write("\nhost free_gb no fim = %s | qemu_n no fim = %s\n" % (last["free_gb"], last["qemu_n"]))
print("monitor concluido: %s" % SUMMARY)
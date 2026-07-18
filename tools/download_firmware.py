#!/usr/bin/env python3
"""Sync GPU firmware from linux-firmware clone → firmware/ + target/firmware/.

Uso:
  python tools/download_firmware.py              # copia do clone (não reclona se existir)
  python tools/download_firmware.py --pull        # git pull no clone
  python tools/download_firmware.py --gsp-ver 570 # pin GSP alternativo
  python tools/download_firmware.py --list        # só inventário

Destinos:
  firmware/          — catálogo lab git-tracked (MIT/redistrib via WHENCE)
                       SEM GSP (gitignore + script; ~75MB economizados no repo)
  target/firmware/   — espelho de build + GSP pinado (única casa dos blobs GSP)

FAT/QEMU:
  NÃO embute o catálogo inteiro. mkfat32.py respeita FW_FAT_CHIPS
  (ex: FW_FAT_CHIPS=gp108,tu102,green_sardine,dg2). Default sem GSP.
  GSP no FAT exige opt-in E blobs em target/ (ou cópia manual).

GSP pin:
  Default 535.113.01 (menor; Nouveau/open-gpu documentado).
  570.144 é opt-in — ABI sem garantia estável; decisão de bring-up.

Lab targets:
  NVIDIA 1050  → nvidia/gp108/gr (FECS+GPCCS+sw_*) [firmware/]
  NVIDIA 2060  → nvidia/tu102/gsp (pin) [target/] + nvidia/tu106/gr [firmware/]
  NVIDIA 4070S → nvidia/ad102/gsp + ga102/gsp [target/ only]
                 (WHENCE: ad103/ad104 → ad102; ad102/gsp/*.bin → ga102)
  AMD 5600G    → amdgpu/green_sardine_*
  AMD 7700G    → amdgpu/gc_10_3_6_* + psp_13_0_5_* (+ sdma_5_2_6)
  AMD Strix+   → amdgpu/gc_11_5_0_* (MES)
  Intel Gen9   → i915/skl_* + kbl_* (já no tree)
  Intel Arc    → i915/dg2_* + xe/*
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO_FW = ROOT / "firmware"
TARGET = ROOT / "target" / "firmware"
GIT_URL = "https://gitlab.com/kernel-firmware/linux-firmware.git"
GIT_DIR = TARGET / "linux-firmware"

# Bring-up pin — override with --gsp-ver
DEFAULT_GSP_VER = "535.113.01"

# ---------------------------------------------------------------------------
# Explicit globs / names (best-effort: missing = skip, not fatal)
# ---------------------------------------------------------------------------

def nvidia_specs(gsp_ver: str) -> list[tuple[str, str]]:
    """(rel_src_under_clone, rel_dst_under_firmware) — files or dirs."""
    specs: list[tuple[str, str]] = []

    # Pascal GP108 — full GR (FECS/GPCCS + sw_*) + ACR HS stubs
    for name in (
        "fecs_bl.bin", "fecs_data.bin", "fecs_inst.bin", "fecs_sig.bin",
        "gpccs_bl.bin", "gpccs_data.bin", "gpccs_inst.bin", "gpccs_sig.bin",
        "sw_bundle_init.bin", "sw_ctx.bin", "sw_method_init.bin", "sw_nonctx.bin",
    ):
        specs.append((f"nvidia/gp108/gr/{name}", f"nvidia/gp108/gr/{name}"))
        # Flat compat for firmware.rs loaders (FW_FECS_*)
        if not name.startswith("sw_"):
            specs.append((f"nvidia/gp108/gr/{name}", f"nvidia/gp108/{name}"))
        else:
            specs.append((f"nvidia/gp108/gr/{name}", f"nvidia/gp108/{name}"))

    # ACR HS — WHENCE: gp108/acr/* → gp102/acr/* (Windows clone sem symlink)
    for name in ("bl.bin", "ucode_load.bin", "ucode_unload.bin", "unload_bl.bin"):
        specs.append((f"nvidia/gp102/acr/{name}", f"nvidia/gp108/acr/{name}"))

    # Turing — shared GSP lives under tu102; tu104/tu106 link → tu102 (WHENCE)
    for name in (
        f"bootloader-{gsp_ver}.bin",
        f"booter_load-{gsp_ver}.bin",
        f"booter_unload-{gsp_ver}.bin",
        f"gsp-{gsp_ver}.bin",
    ):
        specs.append((f"nvidia/tu102/gsp/{name}", f"nvidia/tu102/gsp/{name}"))

    # Turing GR (RTX 2060 = TU106) — local files; fecs_bl links to tu102
    for name in (
        "fecs_data.bin", "fecs_inst.bin", "fecs_sig.bin",
        "gpccs_data.bin", "gpccs_inst.bin", "gpccs_sig.bin",
        "sw_bundle_init.bin", "sw_ctx.bin", "sw_method_init.bin",
        "sw_nonctx.bin", "sw_veid_bundle_init.bin",
    ):
        specs.append((f"nvidia/tu106/gr/{name}", f"nvidia/tu106/gr/{name}"))
    specs.append(("nvidia/tu102/gr/fecs_bl.bin", "nvidia/tu106/gr/fecs_bl.bin"))
    specs.append(("nvidia/tu102/gr/gpccs_bl.bin", "nvidia/tu106/gr/gpccs_bl.bin"))

    # Ada 4070S — WHENCE: ad104→ad102; gsp blob → ga102
    for name in (
        f"bootloader-{gsp_ver}.bin",
        f"booter_load-{gsp_ver}.bin",
        f"booter_unload-{gsp_ver}.bin",
    ):
        specs.append((f"nvidia/ad102/gsp/{name}", f"nvidia/ad102/gsp/{name}"))
    specs.append(
        (f"nvidia/ga102/gsp/gsp-{gsp_ver}.bin", f"nvidia/ga102/gsp/gsp-{gsp_ver}.bin")
    )
    # Materialize WHENCE link so bare-metal loader finds ad102/gsp/gsp-*.bin
    specs.append(
        (f"nvidia/ga102/gsp/gsp-{gsp_ver}.bin", f"nvidia/ad102/gsp/gsp-{gsp_ver}.bin")
    )
    # ga102 booter (same pin) — useful if DID maps Ampere path
    for name in (
        f"bootloader-{gsp_ver}.bin",
        f"booter_load-{gsp_ver}.bin",
        f"booter_unload-{gsp_ver}.bin",
    ):
        specs.append((f"nvidia/ga102/gsp/{name}", f"nvidia/ga102/gsp/{name}"))

    return specs


def amd_specs() -> list[tuple[str, str]]:
    specs: list[tuple[str, str]] = []
    # 5600G Cezanne
    for name in (
        "green_sardine_asd.bin", "green_sardine_ce.bin", "green_sardine_dmcub.bin",
        "green_sardine_me.bin", "green_sardine_mec.bin", "green_sardine_mec2.bin",
        "green_sardine_pfp.bin", "green_sardine_rlc.bin", "green_sardine_sdma.bin",
        "green_sardine_ta.bin", "green_sardine_vcn.bin",
    ):
        specs.append((f"amdgpu/{name}", f"amdgpu/{name}"))

    # 7700G Raphael (gfx1036)
    for name in (
        "gc_10_3_6_ce.bin", "gc_10_3_6_me.bin", "gc_10_3_6_mec.bin",
        "gc_10_3_6_mec2.bin", "gc_10_3_6_pfp.bin", "gc_10_3_6_rlc.bin",
        "psp_13_0_5_asd.bin", "psp_13_0_5_ta.bin", "psp_13_0_5_toc.bin",
        "sdma_5_2_6.bin",
    ):
        specs.append((f"amdgpu/{name}", f"amdgpu/{name}"))

    # Strix / gc_11_5_0 + MES
    for name in (
        "gc_11_5_0_imu.bin", "gc_11_5_0_me.bin", "gc_11_5_0_mec.bin",
        "gc_11_5_0_mes1.bin", "gc_11_5_0_mes_2.bin", "gc_11_5_0_pfp.bin",
        "gc_11_5_0_rlc.bin",
    ):
        specs.append((f"amdgpu/{name}", f"amdgpu/{name}"))

    return specs


def intel_specs() -> list[tuple[str, str]]:
    specs: list[tuple[str, str]] = []
    # Gen9 HD 620/630 — keep current SKL/KBL set
    for name in (
        "skl_dmc_ver1_27.bin", "skl_guc_70.1.1.bin", "skl_huc_2.0.0.bin",
        "kbl_dmc_ver1_04.bin", "kbl_guc_70.1.1.bin", "kbl_huc_4.0.0.bin",
    ):
        specs.append((f"i915/{name}", f"i915/{name}"))

    # Arc DG2
    for name in (
        "dg2_dmc_ver2_08.bin", "dg2_guc_70.bin", "dg2_guc_70.4.1.bin",
        "dg2_huc_gsc.bin",
    ):
        specs.append((f"i915/{name}", f"i915/{name}"))

    # xe/ (Battlemage / Lunar Lake / Panther Lake — Arc novo)
    for name in (
        "bmg_guc_70.bin", "bmg_huc.bin",
        "lnl_guc_70.bin", "lnl_huc.bin", "lnl_gsc_1.bin",
        "ptl_guc_70.bin", "ptl_huc.bin", "ptl_gsc_1.bin",
    ):
        specs.append((f"xe/{name}", f"xe/{name}"))

    return specs


# Chips aceitos por FW_FAT_CHIPS (substring match no rel path)
FAT_CHIP_PATHS = {
    "gp108": ("nvidia/gp108/",),
    "tu102": ("nvidia/tu102/",),
    "tu106": ("nvidia/tu106/",),
    "ad102": ("nvidia/ad102/",),
    "ga102": ("nvidia/ga102/",),
    "green_sardine": ("amdgpu/green_sardine_",),
    "raphael": ("amdgpu/gc_10_3_6_", "amdgpu/psp_13_0_5_", "amdgpu/sdma_5_2_6"),
    "gc_11_5_0": ("amdgpu/gc_11_5_0_",),
    "skl": ("i915/skl_",),
    "kbl": ("i915/kbl_",),
    "dg2": ("i915/dg2_",),
    "xe": ("xe/",),
}


def ensure_clone(pull: bool) -> None:
    git_bin = shutil.which("git")
    if not git_bin:
        print("[ERRO] git nao encontrado.")
        sys.exit(1)
    TARGET.mkdir(parents=True, exist_ok=True)
    if not GIT_DIR.exists():
        print("[GIT] Clonando linux-firmware (shallow)...")
        r = subprocess.run(
            [git_bin, "clone", "--depth", "1", GIT_URL, str(GIT_DIR)],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            print(f"[ERRO] git clone falhou: {r.stderr[:300]}")
            sys.exit(1)
    elif pull:
        print("[GIT] git pull --ff-only...")
        subprocess.run(
            [git_bin, "-C", str(GIT_DIR), "pull", "--ff-only"],
            check=False,
        )
    else:
        print(f"[GIT] reusando clone em {GIT_DIR}")


def is_gsp_path(dst_rel: str) -> bool:
    """GSP blobs nunca entram no repo firmware/ (gitignore + política)."""
    return "/gsp/" in dst_rel.replace("\\", "/")


def copy_one(src_rel: str, dst_rel: str, dest_roots: list[Path]) -> tuple[bool, int]:
    src = GIT_DIR / src_rel
    if not src.is_file():
        return False, 0
    size = src.stat().st_size
    for root in dest_roots:
        dst = root / dst_rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    return True, size


def purge_repo_gsp() -> int:
    """Remove qualquer GSP que tenha vazado para firmware/ (repo)."""
    removed = 0
    if not REPO_FW.exists():
        return 0
    for gsp_dir in REPO_FW.glob("nvidia/*/gsp"):
        if gsp_dir.is_dir():
            for f in gsp_dir.rglob("*"):
                if f.is_file():
                    f.unlink(missing_ok=True)
                    removed += 1
            # limpa dirs vazios
            try:
                gsp_dir.rmdir()
            except OSError:
                pass
    return removed


def write_policy(gsp_ver: str, roots: list[Path]) -> None:
    policy = (
        "# Gerado por download_firmware.py\n"
        "# GSP: SO em target/firmware/nvidia/*/gsp/ -- NAO versionar em firmware/\n"
        "# FAT: NAO embutir GSP sem FW_FAT_CHIPS=tu102,ad102,ga102\n"
        f"GSP_PIN={gsp_ver}\n"
        "GSP_LOCATION=target/firmware/nvidia/{tu102,ad102,ga102}/gsp/\n"
        "FAT_DEFAULT_CHIPS=gp108,skl,kbl,green_sardine\n"
        "FAT_GSP_CHIPS=tu102,ad102,ga102\n"
        f"NOTE=GSP pin ABI {gsp_ver}; WHENCE MIT/redistrib; 570=opt-in bring-up\n"
    )
    for root in roots:
        try:
            (root / "FW_POLICY.txt").write_text(policy, encoding="utf-8")
        except OSError:
            pass


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pull", action="store_true", help="git pull no clone existente")
    ap.add_argument("--gsp-ver", default=DEFAULT_GSP_VER,
                    help=f"Pin GSP (default {DEFAULT_GSP_VER}; alt 570.144)")
    ap.add_argument("--no-repo", action="store_true", help="nao escrever em firmware/")
    ap.add_argument("--no-target", action="store_true", help="nao espelhar em target/firmware/")
    ap.add_argument("--list", action="store_true", help="so listar specs / existencia no clone")
    args = ap.parse_args()

    specs = nvidia_specs(args.gsp_ver) + amd_specs() + intel_specs()

    if args.list:
        ensure_clone(False)
        ok = miss = 0
        bytes_ok = 0
        for src_rel, dst_rel in specs:
            p = GIT_DIR / src_rel
            tag = "GSP->target" if is_gsp_path(dst_rel) else "repo+target"
            if p.is_file():
                ok += 1
                bytes_ok += p.stat().st_size
                print(f"  [OK] {src_rel} -> {dst_rel} ({p.stat().st_size}) [{tag}]")
            else:
                miss += 1
                print(f"  [--] {src_rel}")
        print(f"\n[LIST] {ok} ok / {miss} miss / {bytes_ok/1e6:.1f} MB no clone (GSP pin={args.gsp_ver})")
        print(f"[FAT]  set FW_FAT_CHIPS={','.join(FAT_CHIP_PATHS)}  (default mkfat32: sem GSP)")
        return

    ensure_clone(args.pull)

    use_repo = not args.no_repo
    use_target = not args.no_target
    if not use_repo and not use_target:
        print("[ERRO] nenhum destino (--no-repo e --no-target)")
        sys.exit(1)

    # GSP nunca no repo; se --no-target, GSP e skip (com aviso)
    if use_repo and not use_target:
        print("[AVISO] --no-target: GSP sera IGNORADO (politica: GSP so em target/firmware/)")

    ok = miss = gsp_ok = 0
    total_bytes = 0
    for src_rel, dst_rel in specs:
        gsp = is_gsp_path(dst_rel)
        roots: list[Path] = []
        if gsp:
            if use_target:
                roots = [TARGET]
            else:
                miss += 1
                print(f"  [SKIP-GSP] {dst_rel} (precisa target/)")
                continue
        else:
            if use_repo:
                roots.append(REPO_FW)
            if use_target:
                roots.append(TARGET)
        if not roots:
            continue
        hit, sz = copy_one(src_rel, dst_rel, roots)
        if hit:
            ok += 1
            total_bytes += sz
            if gsp:
                gsp_ok += 1
            where = "target-only" if gsp else "repo+target"
            print(f"  [OK] {dst_rel} ({sz} B) [{where}]")
        else:
            miss += 1
            print(f"  [--] missing clone:{src_rel}")

    purged = purge_repo_gsp()
    policy_roots = []
    if use_repo:
        policy_roots.append(REPO_FW)
    if use_target:
        policy_roots.append(TARGET)
    write_policy(args.gsp_ver, policy_roots)

    print(f"\n[OK] {ok} copiados ({gsp_ok} GSP em target/), {miss} ausentes, {total_bytes/1e6:.1f} MB")
    if purged:
        print(f"[PURGE] {purged} arquivos GSP removidos de firmware/ (repo)")
    print(f"[GSP] pin={args.gsp_ver} location=target/firmware/nvidia/*/gsp/")
    print("[FAT] default exclui */gsp/*; opt-in: FW_FAT_CHIPS=tu102,ad102,ga102")


if __name__ == "__main__":
    main()

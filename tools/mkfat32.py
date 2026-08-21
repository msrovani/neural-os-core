#!/usr/bin/env python3
"""Cria imagem FAT32 com MBR, formata, copia modelos .bitnet e CONFIG.TXT.
Uso: python tools/mkfat32.py [--size 3072] [--label NEURAL-OS] [--output target/disk_qemu.raw]

PACK_LLM=850|13|2b|3b|all  — progressivo (default: 850). Ex: PACK_LLM=850,13
  850 → BITNET850; 13 → BITNET13 (~1.3B xl); 2b → BITNET2B; 3b → BITNET3B; all → tudo
FIT_GATE=1 — filtra PACK_LLM via tools/llmfit_pack_filter.py (nunca sobe degrau).
FAT32: partição pode ser 3GB+; limite ~4GB-1 é por *arquivo*. Modelos grandes (>PIO):
  preferir AirLLM (/model GGUF ATA) em vez de PIO full-RAM.
BOOT_MODE via env BOOT_MODE=qemu|hw (default: inferido do nome do arquivo).
"""
import os, struct, sys, argparse

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_SIZE_MB = 3072  # 3GB dados — cabe 850+1.3B+2B; arquivo FAT32 máx ~4GB-1


def _apply_fit_gate_if_enabled() -> None:
    """FIT_GATE=1 → reescreve PACK_LLM com degraus que cabem na RAM host."""
    if os.environ.get("FIT_GATE", "").strip() not in ("1", "true", "yes", "on"):
        return
    tools_dir = os.path.join(ROOT, "tools")
    if tools_dir not in sys.path:
        sys.path.insert(0, tools_dir)
    try:
        import llmfit_pack_filter as fit

        code = fit.run_fit_gate_from_env()
        print(f"[FIT_GATE] applied exit={code} PACK_LLM={os.environ.get('PACK_LLM')}")
    except Exception as e:
        print(f"[FIT_GATE] ERROR skipped: {e}")


def pack_llm_set() -> set[str]:
    """Tokens normalizados: 850, 13, 2b, 3b. Default só 850 (primeiro degrau)."""
    _apply_fit_gate_if_enabled()
    raw = os.environ.get("PACK_LLM", "850").strip().lower()
    if not raw or raw in ("none", "0", "off"):
        return set()
    if raw in ("all", "*"):
        return {"850", "13", "2b", "3b"}
    out: set[str] = set()
    for tok in raw.replace(";", ",").split(","):
        t = tok.strip().lower().replace(" ", "")
        if t in ("850", "850m", "fast", "large"):
            out.add("850")
        elif t in ("13", "1.3", "1p3", "1.5", "xl", "1.58", "158"):
            out.add("13")
        elif t in ("2b", "2", "2.0"):
            out.add("2b")
        elif t in ("3b", "3", "pro"):
            out.add("3b")
    return out

def find_file(name):
    # target1 = canônico de modelos v6 (SESSION_254+); depois staging/legado.
    # D:\modelos = repo externo de modelos (SESSION_275+).
    import platform
    ext_model_dir = r"D:\modelos" if platform.system() == "Windows" else "/mnt/d/modelos"
    for d in [os.path.join(ROOT, "target1"), os.path.join(ROOT, "models"), ROOT,
              os.path.join(ROOT, "target"), os.path.join(ROOT, "firmware"),
              os.path.join(ROOT, "crates/neural-kernel"), os.path.join(ROOT, "tools/target"),
              ext_model_dir]:
        p = os.path.join(d, name)
        if os.path.exists(p): return p
    return None

def find_large(name, min_bytes=1_000_000):
    """Como find_file, mas exige tamanho mínimo (evita stub MICRO.BITNET ~13KB)."""
    p = find_file(name)
    if p and os.path.getsize(p) >= min_bytes:
        return p
    return None

def find_bitnet_13():
    return find_large("BITNET13.BIN") or find_large("bitnet_1p3b.bitnet")


def find_bitnet_850():
    return (
        find_large("BITNET850.BIN")
        or find_large("bitnet_850m.bitnet")
        or find_large("MICRO.BIN")
    )

def find_bitnet_3b():
    return find_large("BITNET3B.BIN") or find_large("bitnet_3B.bitnet")

def align_up(v, a): return (v + a - 1) // a * a

def create_fat32(path, size_mb, label):
    size = size_mb * 1024 * 1024
    bps, spc, reserved, fat_count = 512, 1, 32, 2
    total_sectors = size // bps
    part_lba = 2048
    part_sectors = total_sectors - part_lba
    # FAT32: geometria relativa a particao (nao ao disco inteiro)
    data_sectors = part_sectors - reserved
    fat_sectors = align_up(((data_sectors * 4) + bps - 1) // bps, 1)
    data_start = reserved + fat_count * fat_sectors
    data_sectors = part_sectors - data_start
    total_clusters = data_sectors // spc
    # FAT32 needs >= 65525 clusters
    if total_clusters < 65525:
        fat_sectors = align_up(fat_sectors * 2, 1)
        data_start = reserved + fat_count * fat_sectors
        data_sectors = part_sectors - data_start
        total_clusters = data_sectors // spc
    # Create file (sparse: seek to last byte, avoids 5GB+ Python memory allocation)
    with open(path, "wb") as f:
        f.seek(size - 1)
        f.write(b'\x00')
    # Write MBR
    mbr = bytearray(512)
    mbr[0x1BE] = 0x00; mbr[0x1BF] = 0x01; mbr[0x1C0] = 0x01; mbr[0x1C1] = 0x00
    mbr[0x1C2] = 0x0C  # FAT32 LBA
    mbr[0x1C3] = 0xFE; mbr[0x1C4] = 0xFF; mbr[0x1C5] = 0xFF
    struct.pack_into("<I", mbr, 0x1C6, part_lba)  # LBA start
    struct.pack_into("<I", mbr, 0x1CA, part_sectors)  # sectors
    mbr[0x1FE], mbr[0x1FF] = 0x55, 0xAA
    with open(path, "r+b") as f:
        f.write(mbr)
    # Write BPB at LBA 2048 (partition start)
    bpb = bytearray(512)
    # jmp + OEM — sem isso o Windows recusa montar FAT32
    bpb[0], bpb[1], bpb[2] = 0xEB, 0x58, 0x90
    bpb[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", bpb, 0x0B, bps)      # bytes per sector
    bpb[0x0D] = spc                             # sectors per cluster
    struct.pack_into("<H", bpb, 0x0E, reserved) # reserved sectors
    bpb[0x10] = fat_count                       # FAT count
    struct.pack_into("<H", bpb, 0x11, 0)        # root entries (0 for FAT32)
    struct.pack_into("<H", bpb, 0x13, 0)        # total sectors 16-bit (0 for FAT32)
    bpb[0x15] = 0xF8                            # media descriptor (hard disk)
    struct.pack_into("<H", bpb, 0x16, 0)        # FAT size 16 (0 for FAT32)
    struct.pack_into("<H", bpb, 0x18, 63)       # sectors per track (CHS dummy)
    struct.pack_into("<H", bpb, 0x1A, 255)      # heads
    struct.pack_into("<I", bpb, 0x1C, part_lba) # hidden sectors (= LBA start)
    struct.pack_into("<I", bpb, 0x20, part_sectors)  # total sectors na particao
    struct.pack_into("<I", bpb, 0x24, fat_sectors)    # FAT size 32
    bpb[0x28] = 0                                # extended flags
    bpb[0x29] = 0                                # FS version
    struct.pack_into("<I", bpb, 0x2C, 2)         # root cluster
    struct.pack_into("<H", bpb, 0x30, 1)         # FSInfo sector
    struct.pack_into("<H", bpb, 0x32, 6)         # backup boot sector
    bpb[0x40] = 0x80                             # drive number
    bpb[0x41] = 0                                # reserved
    bpb[0x42] = 0x29                             # boot signature
    import random
    struct.pack_into("<I", bpb, 0x43, random.randint(0, 0xFFFFFFFF))
    label_bytes = label.encode().ljust(11, b' ')[:11]
    bpb[0x47:0x47+11] = label_bytes
    bpb[0x52:0x52+8] = b'FAT32   '
    bpb[0x1FE], bpb[0x1FF] = 0x55, 0xAA
    with open(path, "r+b") as f:
        f.seek(part_lba * 512)
        f.write(bpb)
        # FSInfo sector
        fsinfo = bytearray(512)
        struct.pack_into("<I", fsinfo, 0, 0x41615252)
        struct.pack_into("<I", fsinfo, 484, 0x61417272)
        struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
        struct.pack_into("<I", fsinfo, 492, total_clusters - 1)
        struct.pack_into("<I", fsinfo, 508, 0xAA550000)
        f.write(fsinfo)
        # Backup boot + FSInfo (@ reserved sectors 6 and 7)
        f.seek((part_lba + 6) * 512)
        f.write(bpb)
        f.write(fsinfo)
        # Zero FAT tables
        for i in range(fat_count):
            f.seek((part_lba + reserved + i * fat_sectors) * 512)
            fat = bytearray(fat_sectors * 512)
            # FAT[0] = media descriptor + dirty flags
            struct.pack_into("<I", fat, 0, 0x0FFFFFF8)
            struct.pack_into("<I", fat, 4, 0x0FFFFFFF)
            # FAT[2] = root directory (EOC) — nunca alocar como data
            struct.pack_into("<I", fat, 8, 0x0FFFFFFF)
            f.write(fat)
        # Zero root directory cluster (cluster 2)
        root_lba = part_lba + data_start + (2 - 2) * spc
        f.seek(root_lba * 512)
        f.write(b'\x00' * spc * bps)
        print(f"[OK] {path}: {size_mb}MB, {total_clusters} clusters, FAT {fat_sectors}s x {fat_count}")

def _inject_device_legos(files: list) -> None:
    tools_dir = os.path.join(ROOT, "tools")
    if tools_dir not in sys.path:
        sys.path.insert(0, tools_dir)
    try:
        from pack_device_legos import append_lego_files

        append_lego_files(files)
    except Exception as e:
        print(f"[LEGO] ERROR inject skipped: {e}")


def populate(path):
    llm = pack_llm_set()
    print(f"[PACK_LLM] {sorted(llm) or 'none'} (env PACK_LLM; default=850)")
    files = [
        ("BGE.BIN", find_file("BGE_M3.BIN") or find_file("bge-small.bitnet") or find_file("bge.bin") or find_file("BGE.BIN")),
        # ADR-0083 §5.3: roteador MoE treinado (tools/train_router.py). Opcional —
        # sem ele o boot usa fallback determinístico com log honesto.
        ("ROUTER.BITNET", find_file("ROUTER.BITNET")),
        ("RUSTCDR.BITNET", find_file("rust_coder.bitnet") or find_file("RUSTCDR.BITNET") or find_file("RUSTCDR2.BIN")),
        ("HW_EXPERT.BITNET", find_file("hw_expert_v6.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("hw_expert_v3.bitnet")),
        ("HWEXPRT.BIN", find_file("hw_expert_v3.bitnet") or find_file("hw_expert_tf.bitnet") or find_file("HWEXPRT.BIN")),
        ("HWEXPRT4.BIN", find_file("hw_expert_v6.bitnet") or find_file("hw_expert_v4.bitnet") or find_file("HWEXPRT4.BIN")),
        ("HWEXPRT.v6", find_file("hw_expert_v6.bitnet") or find_file("HWEXPRT.v6")),
        ("PIPER.BIN", find_file("PIPER_PT_BR.BIN") or find_file("PIPER.BIN") or find_file("PIPER_PT_BR_CADU_MEDIUM.bitnet")),
        ("PIPER_EN.BIN", find_file("PIPER_EN.BIN")),
        ("STT.BIN", find_file("STT.BIN")),
        ("BPE.BIN", find_file("bpe_vocab.bin") or find_file("BPE.BIN")),
        # Progressivo: PACK_LLM=850 → 13 → 2b → 3b (AirLLM p/ GGUF grandes no boot)
        ("BITNET13.BIN", find_bitnet_13() if "13" in llm else None),
        ("BITNET850.BIN", find_bitnet_850() if "850" in llm else None),
        ("MICRO.BIN", None),  # evita stub; boot usa BITNET850/13
        ("BITNET2B.BIN", (find_file("BITNET2B.v6") or find_file("bitnet_2B.bitnet") or find_file("BITNET2B.BIN")
         or find_file("BITNET-2B.BITNET") or find_file("bitnet-BitNet-b1_58-2B-4T.bitnet"))
         if "2b" in llm else None),
        ("BITNET.BIN", None),  # alias legado; não empacota stub
        ("BITNET3B.BIN", find_bitnet_3b() if "3b" in llm else None),
        ("MICRO.BITNET", None if ("850" in llm or "13" in llm) else find_file("MICRO.BITNET")),
        # ADR-0078/0079: todos os slots ModelHub (fat_names_for em cortex::model_hub)
        ("VISION.BIN", find_file("VISION.v6") or find_file("VISION.BIN")),
        ("LLAMA8B.BIN", find_file("PRO.v6") or find_file("LLAMA8B.BIN") or find_file("LLAMA8B.BITNET")),
        ("RUSTCDR3.BIN", find_file("RUSTCDR3.v6") or find_file("RUSTCDR3.BIN") or find_file("RUSTCDR3.BITNET")),
        ("RERANKER.BIN", find_file("RERANKER.v6") or find_file("RERANKER.BIN") or find_file("RERANKER.BITNET")),
        ("LEARNER.BIN", find_file("LEARNER.v6") or find_file("LEARNER.BIN") or find_file("LEARNER.BITNET")),
        ("AGENT.BIN", find_file("AGENT.v6") or find_file("AGENT.BIN") or find_file("AGENT.BITNET")),
        # GOAL3: MicroPython WASM (tools/build_micropython_wasm.py → models/MICROPY.WASM)
        ("MICROPY.WASM", find_file("MICROPY.WASM") or find_file("micropython.wasm")),
    ]
    # ADR-0056: LEGOs cedo (antes do walk firmware) — evita esgotar root dir
    _inject_device_legos(files)
    # SKIP_2B removido: com imagem 512MB+ e pendrive 32GB, sempre inclui se existir
    # Firmware blobs — política FAT (GSP dezenas de MB; não embutir catálogo inteiro).
    # FW_FAT_CHIPS=gp108,skl,kbl,green_sardine  (default)
    # Opt-in GSP: FW_FAT_CHIPS=...,tu102,ad102,ga102
    # FW_FAT_CHIPS=all → legado (tudo em firmware/)
    fw_root = os.path.join(ROOT, "firmware")
    fat_chips = os.environ.get("FW_FAT_CHIPS", "gp108,skl,kbl,green_sardine").strip().lower()
    chip_allow = {c.strip() for c in fat_chips.split(",") if c.strip()} if fat_chips != "all" else None
    # Mapa chip → substrings no path relativo (posix)
    chip_paths = {
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
        # WiFi/NIC legado (se chip listado ou all)
        "rtl_nic": ("rtl_nic/",),
        "rtlwifi": ("rtlwifi/",),
        "iwlwifi": ("intel/iwlwifi/",),
        "ath10k": ("ath10k/",),
    }

    def fw_allowed(rel_posix: str) -> bool:
        if chip_allow is None:
            return True
        # NIC/WiFi legado: sempre no FAT (não é GSP)
        if rel_posix.startswith(
            ("rtl_nic/", "rtlwifi/", "intel/iwlwifi/", "realtek/", "ath10k/")
        ):
            return True
        # GSP: opt-in explícito (dezenas de MB)
        if "/gsp/" in rel_posix:
            return bool(chip_allow & {"tu102", "ad102", "ga102", "tu106"})
        for chip in chip_allow:
            for prefix in chip_paths.get(chip, ()):
                if rel_posix.startswith(prefix) or prefix.rstrip("_") in rel_posix:
                    return True
        return False

    def collect_fw_from(fw_base: str, gsp_only: bool = False) -> None:
        if not os.path.isdir(fw_base):
            return
        for root, dirs, fnames in os.walk(fw_base):
            # Skip clone tree se existir sob target/firmware
            dirs[:] = [d for d in dirs if d != "linux-firmware"]
            for name in fnames:
                ext = os.path.splitext(name)[1].lower()
                if ext not in (".bin", ".fw", ".ucode"):
                    continue
                rel = os.path.relpath(root, fw_base)
                if rel == ".":
                    rel_posix = name
                else:
                    rel_posix = rel.replace("\\", "/") + "/" + name
                if gsp_only and "/gsp/" not in rel_posix:
                    continue
                if not fw_allowed(rel_posix):
                    continue
                prefix = "" if rel == "." else rel.upper().replace("\\", "_").replace("/", "_")
                fw_path = os.path.join(root, name)
                # NVIDIA GP108: nomes 8.3 reais (kernel Fat32Reader usa encode_83).
                # Sem isso FW_FECS_BL_BIN vira "FW_FECS_BL_" e o ACR nunca acha o blob.
                gp108_short = {
                    "fecs_bl.bin": "FECS_BL.BIN",
                    "fecs_data.bin": "FECS_DAT.BIN",
                    "fecs_inst.bin": "FECS_INS.BIN",
                    "fecs_sig.bin": "FECS_SIG.BIN",
                    "gpccs_bl.bin": "GPCCS_BL.BIN",
                    "gpccs_data.bin": "GPCCS_DA.BIN",
                    "gpccs_inst.bin": "GPCCS_IN.BIN",
                    "gpccs_sig.bin": "GPCCS_SI.BIN",
                    "sw_ctx.bin": "SW_CTX.BIN",
                    "sw_bundle_init.bin": "SW_BNDL.BIN",
                    "sw_method_init.bin": "SW_MTHD.BIN",
                    "sw_nonctx.bin": "SW_NONC.BIN",
                    "bl.bin": "ACR_BL.BIN",
                    "ucode_load.bin": "ACRLOAD.BIN",
                    "ucode_unload.bin": "ACRUNLD.BIN",
                    "unload_bl.bin": "ACR_UBL.BIN",
                }
                lname = name.lower()
                # Intel iwlwifi API77 — short 8.3 (SESSION_154 / S1 prep).
                iwlwifi_short = {
                    "iwlwifi-cc-a0-77.ucode": "FW_CC77.BIN",
                    "iwlwifi-so-a0-gf-a0-77.ucode": "FW_SOGF.BIN",
                    "iwlwifi-so-a0-hr-b0-77.ucode": "FW_SOHR.BIN",
                    "iwlwifi-ty-a0-gf-a0-77.ucode": "FW_TYGF.BIN",
                    "iwlwifi-qu-b0-hr-b0-77.ucode": "FW_QUHR.BIN",
                }
                # ath10k QCA6174 hw3.0 — short 8.3 (SESSION_160 / Note 1050).
                ath10k_short = {
                    "firmware-6.bin": "AT10K_F6.BIN",
                    "board-2.bin": "AT10K_B2.BIN",
                    "board.bin": "AT10K_BD.BIN",
                }
                if "iwlwifi" in prefix.lower() and lname in iwlwifi_short:
                    fw_name = iwlwifi_short[lname]
                elif "ath10k" in prefix.lower() and lname in ath10k_short:
                    fw_name = ath10k_short[lname]
                elif ("nvidia" in prefix.lower() and "gp108" in prefix.lower()) or prefix in (
                    "NVIDIA_GP108",
                    "NVIDIA_GP108_GR",
                    "NVIDIA_GP108_ACR",
                ):
                    if lname in gp108_short:
                        fw_name = gp108_short[lname]
                    elif prefix in ("NVIDIA_GP108",):
                        fw_name = "FW_" + name.upper().replace(".", "_")
                    else:
                        fw_name = "FW_GP108_" + name.upper().replace(".", "_")
                elif prefix in ("NVIDIA_GP108",):
                    fw_name = "FW_" + name.upper().replace(".", "_")
                elif prefix.startswith("NVIDIA_GP108"):
                    fw_name = "FW_GP108_" + name.upper().replace(".", "_")
                else:
                    fw_name = (
                        f"FW_{prefix}_{name.upper().replace('.', '_')}"
                        if prefix
                        else f"FW_{name.upper().replace('.', '_')}"
                    )
                # Evita duplicar se já coletado de firmware/
                if any(existing[0] == fw_name for existing in files):
                    continue
                files.append((fw_name, fw_path))

    # Catálogo repo (sem GSP — gitignore)
    collect_fw_from(fw_root, gsp_only=False)
    # GSP só em target/firmware; embute no FAT apenas com FW_FAT_CHIPS opt-in
    if chip_allow is None or (chip_allow & {"tu102", "ad102", "ga102", "tu106"}):
        collect_fw_from(os.path.join(ROOT, "target", "firmware"), gsp_only=True)

    # KernelPack NVIDIA/Intel (host packers → target/)
    for nkp_name, fat_name in (
        ("NKP_SM61.BIN", "NKP_SM61.BIN"),
        ("NKP_VECTOR_ADD.BIN", "NKP_VADD.BIN"),
        ("NKP_GEN9.BIN", "NKP_GEN9.BIN"),
        ("NKP_DG2.BIN", "NKP_DG2.BIN"),
        ("NKP_GFX1030.BIN", "NKP_GFX1030.BIN"),
        ("NKP_GFX1103.BIN", "NKP_GFX1103.BIN"),
        ("NKP_GFX90C.BIN", "NKP_GFX90C.BIN"),
    ):
        nkp_path = os.path.join(ROOT, "target", nkp_name)
        if os.path.isfile(nkp_path) and not any(e[0] == fat_name for e in files):
            files.append((fat_name, nkp_path))

    # CONFIG.TXT — BOOT_MODE via env ou inferido do path (passado por populate caller)
    boot_mode = os.environ.get("BOOT_MODE", "hw").strip().lower()
    if boot_mode not in ("qemu", "hw"):
        boot_mode = "hw"
    platform = "virtio-qemu" if boot_mode == "qemu" else "baremetal"
    config_content = (
        f"BOOT_MODE={boot_mode}\nPLATFORM={platform}\nGPU=auto\nLOG_TO_FAT32=1\n"
        + (
            "MODELS_SOURCE=network\n"
            if os.environ.get("MODELS_SOURCE", "").strip().lower() == "network"
            else ""
        )
    ).encode()
    files.append(("CONFIG.TXT", config_content))
    # UPDATE.CFG — endereço do servidor OTA (ADR-0086). Override via env UPDATE_URL
    # (ex: note 1 no cabo/ICS: UPDATE_URL=http://192.168.137.1:8080/UPDATE.MANIFEST).
    update_url = os.environ.get("UPDATE_URL", "http://10.0.2.2:8080/UPDATE.MANIFEST").strip()
    files.append(("UPDATE.CFG", f"UPDATE_URL={update_url}\n".encode()))
    # BOOT.LOG pre-alocado (256 KiB): canal DEV/TEST (Live stick / QEMU).
    # Produto Installed usa /logs/boot_<tick>.log com timestamp (SESSION_270).
    boot_log = (
        b"[S] neural-os-core BOOT.LOG (DEV/TEST only)\n"
        b"# Placeholder - apos soft-reboot HW deve ter linhas [T+] / Knn:\n"
        b"# Se ainda so isto: UEFI nao achou SFS do volume de dados.\n"
        + b"\x00" * (256 * 1024 - 180)
    )
    files.append(("BOOT.LOG", boot_log[: 256 * 1024]))

    with open(path, "r+b") as f:
        f.seek(2048 * 512)  # skip MBR + partition start
        bpb = bytearray(f.read(512))
        bps = struct.unpack_from("<H", bpb, 0x0B)[0]
        spc = bpb[0x0D]
        reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
        fat_count = bpb[0x10]
        fat_sectors = struct.unpack_from("<I", bpb, 0x24)[0]
        part_sectors = struct.unpack_from("<I", bpb, 0x20)[0]
        root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
        data_lba = 2048 + reserved + fat_count * fat_sectors
        fat_lba = 2048 + reserved
        # Calculate total_clusters from partition sectors
        data_sectors = part_sectors - reserved - fat_count * fat_sectors
        total_clusters = data_sectors // spc

        for name, src in files:
            data = src if isinstance(src, bytes) else (open(src, "rb").read() if src else None)
            if data is None:
                print(f"  [--] {name} — nao encontrado")
                continue
            clusters_needed = (len(data) + spc * bps - 1) // (spc * bps)
            # Find free clusters in FAT
            free = []
            max_cluster = total_clusters + 2  # root cluster is 2, so add 2 for safety
            max_fat_sector = fat_sectors * fat_count
            for cl in range(2, max_cluster):
                fat_sec_offset = (cl * 4) // bps
                if fat_sec_offset >= max_fat_sector:
                    break  # Beyond FAT table bounds
                fat_sec_lba = fat_lba + fat_sec_offset
                f.seek(fat_sec_lba * 512 + (cl * 4) % bps)
                entry = struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF
                if entry == 0:
                    free.append(cl)
                    if len(free) >= clusters_needed:
                        break
            if len(free) < clusters_needed:
                print(f"  [--] {name}: sem espaco ({len(free)}/{clusters_needed} clusters)")
                continue
            # Write data & FAT chain
            for idx, cl in enumerate(free):
                cl_lba = data_lba + (cl - 2) * spc
                chunk = data[idx * spc * bps : (idx + 1) * spc * bps]
                if chunk:
                    f.seek(cl_lba * 512)
                    f.write(chunk + b'\x00' * (spc * bps - len(chunk)))
                next_cl = free[idx + 1] if idx + 1 < len(free) else 0x0FFFFFFF
                f.seek(fat_lba * 512 + cl * 4)
                f.write(struct.pack("<I", next_cl & 0x0FFFFFFF))
            # Write directory entry into root (extend cluster chain if needed)
            name83 = name.upper().replace(".", "").ljust(11) if False else None
            # 8.3: split name/ext
            if "." in name:
                base, _, ext = name.rpartition(".")
                name_bytes = (base[:8].ljust(8) + ext[:3].ljust(3)).upper().encode("ascii", "replace")
            else:
                name_bytes = name[:11].ljust(11).upper().encode("ascii", "replace")
            entry_data = bytearray(32)
            entry_data[0:11] = name_bytes
            entry_data[11] = 0x20  # archive
            struct.pack_into("<I", entry_data, 28, len(data))
            struct.pack_into("<H", entry_data, 26, free[0] & 0xFFFF)
            struct.pack_into("<H", entry_data, 20, (free[0] >> 16) & 0xFFFF)

            def fat_get(cl):
                fat_sec_offset = (cl * 4) // bps
                if fat_sec_offset >= max_fat_sector:
                    return 0x0FFFFFFF  # Return EOC if beyond FAT bounds
                f.seek(fat_lba * 512 + cl * 4)
                return struct.unpack("<I", f.read(4))[0] & 0x0FFFFFFF

            def fat_set(cl, val):
                f.seek(fat_lba * 512 + cl * 4)
                f.write(struct.pack("<I", val & 0x0FFFFFFF))

            def find_any_free_cluster():
                # start after last used free chain to avoid O(n^2) from 2
                start = free[-1] + 1 if free else 3
                max_cluster = total_clusters + 2  # root cluster is 2, so add 2 for safety
                for cl in range(start, max_cluster):
                    if fat_get(cl) == 0:
                        return cl
                for cl in range(3, start):
                    if fat_get(cl) == 0:
                        return cl
                return None

            placed = False
            dir_cl = root_cluster
            while not placed and dir_cl >= 2 and dir_cl < 0x0FFFFFF8:
                root_lba = data_lba + (dir_cl - 2) * spc
                for offs in range(0, spc * bps, 32):
                    f.seek(root_lba * 512 + offs)
                    first = f.read(1)
                    if first in (b"\x00", b"\xE5"):
                        f.seek(root_lba * 512 + offs)
                        f.write(entry_data)
                        placed = True
                        break
                if placed:
                    break
                nxt = fat_get(dir_cl)
                if nxt >= 0x0FFFFFF8 or nxt < 2:
                    # extend root directory cluster chain
                    new_cl = find_any_free_cluster()
                    if new_cl is None:
                        break
                    fat_set(dir_cl, new_cl)
                    fat_set(new_cl, 0x0FFFFFFF)
                    f.seek((data_lba + (new_cl - 2) * spc) * 512)
                    f.write(b"\x00" * spc * bps)
                    dir_cl = new_cl
                else:
                    dir_cl = nxt

            status = "OK" if placed else "sem slot dir"
            print(f"  [{status}] {name} ({len(data)//1024}K, {clusters_needed} cls)")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--size", default=str(DEFAULT_SIZE_MB),
                        help=f"Size in MB (default: {DEFAULT_SIZE_MB})")
    parser.add_argument("--label", default="NEURAL-OS", help="Volume label")
    parser.add_argument("--output", default=None, help="Output path (default: target/disk_qemu.raw)")
    args = parser.parse_args()
    size_mb = int(args.size.replace("M", "").replace("m", ""))
    target_dir = os.path.join(ROOT, "target")
    os.makedirs(target_dir, exist_ok=True)
    disks = []
    if args.output:
        out = args.output if os.path.isabs(args.output) else os.path.join(ROOT, args.output)
        disks.append(out)
    else:
        disks = [os.path.join(target_dir, "disk_qemu.raw")]
    for d in disks:
        os.makedirs(os.path.dirname(d) or ".", exist_ok=True)
        # Infer BOOT_MODE se ainda nao setado
        if "BOOT_MODE" not in os.environ:
            base = os.path.basename(d).lower()
            os.environ["BOOT_MODE"] = "hw" if "disk_hw" in base else "qemu"
        print(f"\n=== {os.path.basename(d)} ({size_mb}MB) BOOT_MODE={os.environ.get('BOOT_MODE')} ===")
        if os.path.exists(d):
            os.remove(d)
        create_fat32(d, size_mb, args.label)
        populate(d)

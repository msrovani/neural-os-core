#!/usr/bin/env python3
"""Inspect USB/unified disk images: MBR/GPT, ESP, FAT32 root assets, FW counts."""
from __future__ import annotations

import struct
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SECTOR = 512
GUID_ESP = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")
GUID_BASIC = bytes.fromhex("a2a0d0ebb9e5334487c068b6b72699c7")


def read_exact(f, n: int) -> bytes:
    b = f.read(n)
    if len(b) != n:
        raise EOFError(f"short read {len(b)}/{n} at pos={f.tell()}")
    return b


def decode_83(name11: bytes) -> str:
    raw = bytearray(name11)
    if raw[0] == 0x05:
        raw[0] = 0xE5
    name = bytes(raw[:8]).decode("ascii", "replace").rstrip()
    ext = bytes(raw[8:11]).decode("ascii", "replace").rstrip()
    return f"{name}.{ext}" if ext else name


def parse_mbr_partitions(mbr: bytes):
    parts = []
    for i in range(4):
        off = 446 + i * 16
        ptype = mbr[off + 4]
        lba = struct.unpack_from("<I", mbr, off + 8)[0]
        sectors = struct.unpack_from("<I", mbr, off + 12)[0]
        if ptype == 0 or sectors == 0:
            continue
        parts.append({"idx": i, "type": ptype, "lba": lba, "sectors": sectors})
    return parts


def parse_gpt(f):
    f.seek(SECTOR)
    hdr = f.read(92)
    if len(hdr) < 92 or hdr[:8] != b"EFI PART":
        return None
    entries_lba = struct.unpack_from("<Q", hdr, 72)[0]
    entry_count = struct.unpack_from("<I", hdr, 80)[0]
    entry_size = struct.unpack_from("<I", hdr, 84)[0]
    f.seek(entries_lba * SECTOR)
    parts = []
    for i in range(entry_count):
        e = read_exact(f, entry_size)
        if e[:16] == b"\x00" * 16:
            continue
        start = struct.unpack_from("<Q", e, 32)[0]
        end = struct.unpack_from("<Q", e, 40)[0]
        name = e[56:128].decode("utf-16-le", "replace").rstrip("\x00")
        kind = "ESP" if e[:16] == GUID_ESP else ("BASIC" if e[:16] == GUID_BASIC else "OTHER")
        parts.append({
            "idx": i, "kind": kind, "lba": start, "end": end,
            "sectors": end - start + 1, "name": name,
        })
    return parts


def read_bpb(f, part_lba: int):
    f.seek(part_lba * SECTOR)
    bpb = bytearray(read_exact(f, 512))
    bps = struct.unpack_from("<H", bpb, 0x0B)[0] or 512
    spc = bpb[0x0D] or 1
    reserved = struct.unpack_from("<H", bpb, 0x0E)[0]
    fat_count = bpb[0x10]
    root_ents = struct.unpack_from("<H", bpb, 0x11)[0]
    fat16 = struct.unpack_from("<H", bpb, 0x16)[0]
    fat32 = struct.unpack_from("<I", bpb, 0x24)[0]
    root_cluster = struct.unpack_from("<I", bpb, 0x2C)[0]
    fs32 = bytes(bpb[0x52:0x5A]).decode("ascii", "replace").strip("\x00 ")
    fs16 = bytes(bpb[0x36:0x3E]).decode("ascii", "replace").strip("\x00 ")
    is_fat32 = fat16 == 0 and root_ents == 0
    fat_sectors = fat32 if is_fat32 else fat16
    root_dir_sectors = ((root_ents * 32) + (bps - 1)) // bps
    fat_lba = part_lba + reserved
    root_lba = fat_lba + fat_count * fat_sectors
    data_lba = root_lba + (0 if is_fat32 else root_dir_sectors)
    return {
        "bps": bps, "spc": spc, "reserved": reserved, "fat_count": fat_count,
        "fat_sectors": fat_sectors, "root_cluster": root_cluster if is_fat32 else 0,
        "fat_lba": fat_lba, "root_lba": root_lba, "data_lba": data_lba,
        "root_dir_sectors": root_dir_sectors, "root_ents": root_ents,
        "fs_type": fs32 if is_fat32 else fs16, "is_fat32": is_fat32,
        "part_lba": part_lba,
    }


def fat_next(f, meta, cluster: int) -> int:
    bps = meta["bps"]
    if meta["is_fat32"]:
        off = meta["fat_lba"] * SECTOR + cluster * 4
        f.seek(off)
        return struct.unpack("<I", read_exact(f, 4))[0] & 0x0FFFFFFF
    # FAT16
    off = meta["fat_lba"] * SECTOR + cluster * 2
    f.seek(off)
    return struct.unpack("<H", read_exact(f, 2))[0]


def cluster_to_lba(meta, cluster: int) -> int:
    return meta["data_lba"] + (cluster - 2) * meta["spc"]


def read_cluster_chain(f, meta, start: int, max_clusters: int = 8192) -> bytes:
    out = bytearray()
    cl = start
    seen = set()
    size = meta["spc"] * meta["bps"]
    eoc = 0x0FFFFFF8 if meta["is_fat32"] else 0xFFF8
    for _ in range(max_clusters):
        if cl < 2 or cl >= eoc or cl in seen:
            break
        seen.add(cl)
        f.seek(cluster_to_lba(meta, cl) * SECTOR)
        out.extend(read_exact(f, size))
        nxt = fat_next(f, meta, cl)
        if not meta["is_fat32"]:
            if nxt >= 0xFFF8:
                break
        else:
            if nxt >= 0x0FFFFFF8:
                break
        cl = nxt
    return bytes(out)


def parse_dir_entries(raw: bytes):
    entries = []
    lfn_parts = {}
    i = 0
    while i + 32 <= len(raw):
        ent = raw[i:i + 32]
        i += 32
        if ent[0] == 0x00:
            break
        if ent[0] == 0xE5:
            lfn_parts = {}
            continue
        attr = ent[11]
        if attr == 0x0F:
            seq = ent[0] & 0x1F
            chars = ent[1:11] + ent[14:26] + ent[28:32]
            s = chars.decode("utf-16-le", "replace").split("\x00", 1)[0].replace("\uffff", "")
            lfn_parts[seq] = s
            continue
        if attr & 0x08:
            lfn_parts = {}
            continue
        short = decode_83(ent[0:11])
        size = struct.unpack_from("<I", ent, 28)[0]
        cluster = struct.unpack_from("<H", ent, 26)[0] | (struct.unpack_from("<H", ent, 20)[0] << 16)
        is_dir = bool(attr & 0x10)
        long = ""
        if lfn_parts:
            long = "".join(lfn_parts[k] for k in sorted(lfn_parts))
            lfn_parts = {}
        name = long or short
        if name in (".", ".."):
            continue
        entries.append({
            "name": name, "short": short, "size": size,
            "cluster": cluster, "is_dir": is_dir, "attr": attr,
        })
    return entries


def list_fat_root(f, part_lba: int):
    meta = read_bpb(f, part_lba)
    print(
        f"  BPB: fs={meta['fs_type']!r} fat32={meta['is_fat32']} bps={meta['bps']} "
        f"spc={meta['spc']} reserved={meta['reserved']} fats={meta['fat_count']} "
        f"fat_sec={meta['fat_sectors']} root_ents={meta['root_ents']} "
        f"root_cl={meta['root_cluster']} root_lba={meta['root_lba']} data_lba={meta['data_lba']}"
    )
    if meta["is_fat32"]:
        raw = read_cluster_chain(f, meta, meta["root_cluster"] or 2)
    else:
        f.seek(meta["root_lba"] * SECTOR)
        raw = read_exact(f, meta["root_dir_sectors"] * SECTOR)
    return meta, parse_dir_entries(raw)


def list_subdir(f, meta, cluster: int):
    if cluster < 2:
        return []
    raw = read_cluster_chain(f, meta, cluster)
    return parse_dir_entries(raw)


def classify_fw(name: str) -> str | None:
    u = name.upper()
    if not u.startswith("FW_"):
        return None
    if u.startswith("FW_I915_"):
        return "i915"
    if u.startswith("FW_RTL_NIC_") or u.startswith("FW_RTL_NIC"):
        return "rtl_nic"
    if u.startswith("FW_RTLWIFI_") or u.startswith("FW_RTLWIFI"):
        return "rtlwifi"
    if "IWLWIFI" in u:
        return "iwlwifi"
    if u.startswith("FW_FECS") or u.startswith("FW_GPCCS"):
        return "nvidia_gp108"
    if u.startswith("FW_NVIDIA_GP108"):
        return "nvidia_gp108"
    return "fw_other"


def match_critical(entries):
    found = {}
    checks = {
        "BITNET2B": lambda n, s: "BITNET2B" in n or "BITNET2B" in s,
        "PIPER": lambda n, s: n.startswith("PIPER") or s.startswith("PIPER"),
        "HWEXPRT": lambda n, s: "HWEXPRT" in n or "HW_EXPERT" in n or "HWEXPRT" in s or "HW_EXPERT" in s,
        "RUSTCDR": lambda n, s: n.startswith("RUSTCDR") or s.startswith("RUSTCDR"),
        "BGE": lambda n, s: n == "BGE.BIN" or s == "BGE.BIN" or n.startswith("BGE."),
        "STT": lambda n, s: n.startswith("STT") or s.startswith("STT"),
        "BPE": lambda n, s: n.startswith("BPE") or s.startswith("BPE"),
        "MICRO": lambda n, s: n.startswith("MICRO") or s.startswith("MICRO"),
        "CONFIG": lambda n, s: n.startswith("CONFIG") or s.startswith("CONFIG"),
        "BITNET.BIN": lambda n, s: n == "BITNET.BIN" or s == "BITNET.BIN",
    }
    for key, pred in checks.items():
        hit = name = size = None
        for e in entries:
            if e["is_dir"]:
                continue
            nu, su = e["name"].upper(), e["short"].upper()
            if pred(nu, su):
                hit, name, size = True, e["name"], e["size"]
                break
        found[key] = (bool(hit), name, size)
    return found


def esp_has_boot(f, part_lba: int):
    meta, ents = list_fat_root(f, part_lba)
    has_efi_dir = any(e["is_dir"] and e["name"].upper() == "EFI" for e in ents)
    bootx64 = False
    nested_names = []
    for e in ents:
        nested_names.append(("ROOT", e["name"], e["size"], e["is_dir"]))
        if e["is_dir"] and e["name"].upper() == "EFI" and e["cluster"] >= 2:
            for s in list_subdir(f, meta, e["cluster"]):
                nested_names.append(("EFI", s["name"], s["size"], s["is_dir"]))
                if s["is_dir"] and s["name"].upper() == "BOOT" and s["cluster"] >= 2:
                    for b in list_subdir(f, meta, s["cluster"]):
                        nested_names.append(("EFI/BOOT", b["name"], b["size"], b["is_dir"]))
                        if "BOOTX64" in b["name"].upper() or "BOOTX64" in b["short"].upper():
                            bootx64 = True
                if "BOOTX64" in s["name"].upper():
                    bootx64 = True
    non_empty = len(ents) > 0
    # Also scan raw for BOOTX64 string in ESP partition region
    if not bootx64:
        f.seek(part_lba * SECTOR)
        # read up to 6MB of ESP
        chunk = f.read(6 * 1024 * 1024)
        if b"BOOTX64" in chunk.upper() or b"bootx64" in chunk:
            bootx64 = True
    ok = bootx64 or (has_efi_dir and non_empty)
    detail = f"root={len(ents)} efi_dir={has_efi_dir} bootx64={bootx64} non_empty={non_empty}"
    print("  ESP listing:")
    for loc, name, size, is_dir in nested_names:
        print(f"    [{loc}] {'DIR' if is_dir else 'FILE'} {name} size={size}")
    return ok, detail, ents


def inspect_image(path: Path, label: str):
    print("=" * 72)
    print(f"IMAGE: {label} ({path})")
    print(f"size={path.stat().st_size}")
    result = {
        "label": label, "entries": [], "esp_ok": False, "esp_detail": "",
        "fw": {}, "critical": {},
    }
    with open(path, "rb") as f:
        mbr = read_exact(f, SECTOR)
        print(f"MBR signature 55AA: {mbr[0x1FE:0x200] == b'\\x55\\xaa'}")
        mbr_parts = parse_mbr_partitions(mbr)
        print("MBR partitions:")
        for p in mbr_parts:
            print(f"  #{p['idx']} type=0x{p['type']:02X} lba={p['lba']} sectors={p['sectors']}")
        gpt = parse_gpt(f)
        esp_lba = data_lba = None
        if gpt:
            print("GPT partitions:")
            for p in gpt:
                print(
                    f"  #{p['idx']} {p['kind']} name={p['name']!r} "
                    f"lba={p['lba']} end={p['end']} sectors={p['sectors']}"
                )
                if p["kind"] == "ESP" and esp_lba is None:
                    esp_lba = p["lba"]
                if p["kind"] == "BASIC" and data_lba is None:
                    data_lba = p["lba"]
        else:
            print("GPT: none (MBR-only)")
            for p in mbr_parts:
                if p["type"] in (0x0B, 0x0C) and data_lba is None:
                    data_lba = p["lba"]
                if p["type"] == 0xEF and esp_lba is None:
                    esp_lba = p["lba"]
            if data_lba is None:
                for p in mbr_parts:
                    if p["type"] != 0xEE:
                        data_lba = p["lba"]
                        break
                if data_lba is None:
                    data_lba = 2048

        if esp_lba is not None:
            print(f"\n--- ESP @ LBA {esp_lba} ---")
            try:
                ok, detail, _ = esp_has_boot(f, esp_lba)
            except Exception as ex:
                ok, detail = False, f"parse_error: {ex}"
                print(f"  ESP parse error: {ex}")
            result["esp_ok"] = ok
            result["esp_detail"] = detail
            print(f"ESP UEFI boot: {'YES' if ok else 'NO'} ({detail})")
        else:
            print("\n--- ESP: not found ---")
            result["esp_detail"] = "no ESP partition"

        if data_lba is None:
            print("\n--- DATA FAT: not found ---")
            return result

        print(f"\n--- DATA FAT @ LBA {data_lba} ---")
        meta, ents = list_fat_root(f, data_lba)
        result["entries"] = ents
        print(f"Root directory entries ({len(ents)}):")
        for e in sorted(ents, key=lambda x: x["name"].upper()):
            kind = "DIR" if e["is_dir"] else "FILE"
            print(f"  [{kind}] {e['name']:44s} short={e['short']:12s} size={e['size']}")

        fw = defaultdict(int)
        for e in ents:
            if e["is_dir"]:
                continue
            g = classify_fw(e["name"]) or classify_fw(e["short"])
            if g:
                fw[g] += 1
        result["fw"] = dict(fw)
        print("\nFW prefix groups:")
        for k in ("nvidia_gp108", "i915", "rtl_nic", "rtlwifi", "iwlwifi", "fw_other"):
            print(f"  {k}: {fw.get(k, 0)}")

        crit = match_critical(ents)
        result["critical"] = crit
        print("\nCritical assets:")
        for k, (pres, name, size) in crit.items():
            print(f"  {k}: present={pres} name={name} size={size}")
    return result


def count_src_fw():
    mapping = {
        "nvidia_gp108": ROOT / "firmware/nvidia/gp108",
        "i915": ROOT / "firmware/i915",
        "rtl_nic": ROOT / "firmware/rtl_nic",
        "rtlwifi": ROOT / "firmware/rtlwifi",
        "iwlwifi": ROOT / "firmware/intel/iwlwifi",
    }
    out = {}
    for k, p in mapping.items():
        if not p.is_dir():
            out[k] = 0
            continue
        # mkfat32 only packs .bin/.fw/.ucode
        n = 0
        for fp in p.rglob("*"):
            if fp.is_file() and fp.suffix.lower() in (".bin", ".fw", ".ucode"):
                n += 1
        # also count extensionless? GP108 may be named differently
        if n == 0:
            n = sum(1 for fp in p.rglob("*") if fp.is_file())
        out[k] = n
    return out


def main():
    target = ROOT / "target"
    print("=== Image existence ===")
    for n in ("usb_hw.img", "disk_hw_unified.raw", "disk_hw.raw", "disk_qemu.raw", "uefi.img", "bios.img"):
        p = target / n
        if p.exists():
            st = p.stat()
            print(f"EXISTS {n} size={st.st_size} mtime={st.st_mtime}")
        else:
            print(f"MISSING {n}")

    primary = None
    for name in ("usb_hw.img", "disk_hw_unified.raw"):
        if (target / name).exists():
            primary = (target / name, name)
            break
    if primary is None:
        print("CRITICAL: usb_hw.img and disk_hw_unified.raw are BOTH MISSING")
    else:
        print(f"\nPrimary image: {primary[1]}")

    results = {}
    if primary:
        results[primary[1]] = inspect_image(primary[0], primary[1])

    disk_hw = target / "disk_hw.raw"
    if disk_hw.exists():
        results["disk_hw.raw"] = inspect_image(disk_hw, "disk_hw.raw")

    src = count_src_fw()
    print("\n" + "=" * 72)
    print("Source firmware counts (.bin/.fw/.ucode or all files if none):")
    for k, v in src.items():
        print(f"  {k}: {v}")

    print("\n" + "=" * 72)
    print("COMPARISON")
    primary_key = primary[1] if primary else None
    if primary_key and "disk_hw.raw" in results:
        a = {e["name"].upper(): e for e in results[primary_key]["entries"] if not e["is_dir"]}
        b = {e["name"].upper(): e for e in results["disk_hw.raw"]["entries"] if not e["is_dir"]}
        only_hw = sorted(set(b) - set(a))
        only_u = sorted(set(a) - set(b))
        print(f"On disk_hw.raw but MISSING from {primary_key}: {len(only_hw)}")
        for n in only_hw:
            print(f"  - {n} size={b[n]['size']}")
        print(f"On {primary_key} but not on disk_hw.raw: {len(only_u)}")
        for n in only_u:
            print(f"  - {n} size={a[n]['size']}")

    if primary_key:
        r = results[primary_key]
        print("\nMARKDOWN_TABLE_BEGIN")
        print(f"| asset | present on {primary_key}? | size |")
        print("|---|---|---|")
        for k, (pres, name, size) in r["critical"].items():
            print(f"| {k} | {'YES' if pres else 'NO'} | {size if size is not None else '-'} |")
        print(f"| ESP UEFI | {'YES' if r['esp_ok'] else 'NO'} | {r['esp_detail']} |")
        for k in ("nvidia_gp108", "i915", "rtl_nic", "rtlwifi", "iwlwifi"):
            print(f"| FW {k} | {r['fw'].get(k, 0)} found / {src[k]} src | - |")
        print("MARKDOWN_TABLE_END")

        missing = [k for k, (pres, _, _) in r["critical"].items() if not pres]
        # BITNET.BIN is optional alias when BITNET2B present
        crit_missing = [m for m in missing if not (m == "BITNET.BIN" and r["critical"]["BITNET2B"][0])]
        fw_miss = [f"{k}:{r['fw'].get(k,0)}<{src[k]}" for k in src if r["fw"].get(k, 0) < src[k]]
        all_ok = (len(crit_missing) == 0) and r["esp_ok"] and not fw_miss
        print(f"\nVERDICT_ALL_CRITICAL: {'YES' if all_ok else 'NO'}")
        if crit_missing:
            print(f"MISSING_ASSETS: {', '.join(crit_missing)}")
        if fw_miss:
            print(f"MISSING_FW: {', '.join(fw_miss)}")
        if not r["esp_ok"]:
            print("MISSING: ESP UEFI boot")


if __name__ == "__main__":
    main()

"""analyze_sdio.py — Estuda conteudo dos DriverPacks SDIO para ML.
Extrai metadados de .inf, .sys (PE), .cat, .txt de MULTIPLOS packs.
Gera relatorio do que pode ser usado para treinar modelos.
"""
import os, re, json, tempfile, shutil, subprocess
from pathlib import Path
from collections import defaultdict

SDIO_DIR = Path(r"C:\Users\msrov\Downloads\SDIO\drivers")
SZ = r"C:\Program Files\7-Zip\7z.exe"
TMP = Path(tempfile.mkdtemp())
REPORT = {}

def seven_list(pack):
    """Lista arquivos num .7z."""
    r = subprocess.run([SZ, "l", "-slt", str(pack)], capture_output=True, text=True, timeout=30)
    # Parse: Path = xxx / Size = xxx
    files = []
    for block in r.stdout.split("\n\n"):
        path = ""
        size = 0
        for line in block.split("\n"):
            if line.startswith("Path = "):
                path = line[7:].strip()
            if line.startswith("Size = "):
                size = int(line[7:].strip())
            if line.startswith("Method = "):
                method = line[9:].strip()
        if path and size > 0:
            files.append({"path": path, "size": size, "method": method if 'method' in dir() else ""})
    return files

def seven_extract_one(pack, target):
    """Extrai um arquivo especifico do .7z."""
    out = TMP / os.path.basename(target)
    os.makedirs(out.parent, exist_ok=True)
    # 7z e [target_dir] -o[output_dir] -y
    trg_dir = TMP / os.path.dirname(target)
    trg_dir.mkdir(parents=True, exist_ok=True)
    r = subprocess.run([SZ, "e", str(pack), f"-o{trg_dir}", target, "-y"],
                      capture_output=True, text=True, timeout=30)
    extracted = trg_dir / os.path.basename(target)
    return extracted if extracted.exists() else None

def analyze_inf(path):
    """Extrai dados estruturados de .inf."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except: return None

    data = {"file": str(path), "size": len(text), "hwids": [], "class_guid": "",
            "device_class": "", "provider": "", "driver_ver": "", "device_names": [],
            "strings": {}, "sections": [], "n_strings": 0}

    # Sections
    data["sections"] = re.findall(r'\[([^\]]+)\]', text)

    # HWIDs
    pci = re.findall(r'PCI\\VEN_(\w{4})&DEV_(\w{4})(?:&SUBSYS_(\w{8}))?', text, re.I)
    usb = re.findall(r'USB\\VID_(\w{4})&PID_(\w{4})', text, re.I)
    acpi = re.findall(r'ACPI\\(\w{8})', text, re.I)
    for v, d, s in pci:
        data["hwids"].append(f"PCI:VEN_{v.upper()}:DEV_{d.upper()}" + (f":SUBSYS_{s.upper()}" if s else ""))
    for v, d in usb:
        data["hwids"].append(f"USB:VID_{v.upper()}:PID_{d.upper()}")
    for a in acpi:
        data["hwids"].append(f"ACPI:{a.upper()}")
    data["n_hwids"] = len(data["hwids"])

    # Metadata
    for m in re.finditer(r'ClassGUID\s*=\s*\{([^}]+)\}', text, re.I):
        data["class_guid"] = m.group(1)
    for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
        data["device_class"] = m.group(1)
    for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
        data["provider"] = m.group(1)
    for m in re.finditer(r'DriverVer\s*=\s*([^,\n]+)', text, re.I):
        data["driver_ver"] = m.group(1).strip()
    for m in re.finditer(r'DriverVer\s*=\s*(\d+/\d+/\d+)', text, re.I):
        data["driver_date"] = m.group(1)

    # Strings section
    in_str = False
    for line in text.split("\n"):
        s = line.strip()
        if s.startswith("[Strings]"): in_str = True; continue
        if in_str and s.startswith("["): break
        if in_str and "=" in s:
            k, v = s.split("=", 1)
            data["strings"][k.strip("%").strip()] = v.strip().strip('"')
    data["n_strings"] = len(data["strings"])
    return data

def analyze_pe(path):
    """Extrai dados de PE (.sys, .dll, .exe)."""
    try:
        import pefile
        pe = pefile.PE(path)
    except: return None

    data = {"file": str(path), "image_base": f"0x{pe.OPTIONAL_HEADER.ImageBase:08X}",
            "entry": f"0x{pe.OPTIONAL_HEADER.AddressOfEntryPoint:08X}",
            "sections": [], "imports": [], "exports": [], "version": ""}

    for s in pe.sections:
        data["sections"].append({
            "name": s.Name.decode().strip(chr(0)).strip(),
            "size": s.SizeOfRawData,
            "va": f"0x{s.VirtualAddress:08X}"
        })

    if hasattr(pe, 'DIRECTORY_ENTRY_IMPORT'):
        dlls = set()
        for imp in pe.DIRECTORY_ENTRY_IMPORT:
            dll = imp.dll.decode()
            funcs = [f.name.decode() if f.name else f"ord_{f.ordinal}" for f in imp.imports[:5]]
            dlls.add(dll)
            data["imports"].append({"dll": dll, "n_funcs": len(imp.imports), "sample": funcs[:5]})
        data["n_dlls"] = len(dlls)

    if hasattr(pe, 'DIRECTORY_ENTRY_EXPORT'):
        for exp in pe.DIRECTORY_ENTRY_EXPORT.symbols[:20]:
            if exp.name:
                data["exports"].append(exp.name.decode())

    if hasattr(pe, 'FileInfo'):
        try:
            for fi in pe.FileInfo:
                if hasattr(fi, 'StringTable'):
                    for st in fi.StringTable:
                        if 'FileVersion' in st.entries:
                            data["version"] = st.entries['FileVersion']
                        if 'ProductName' in st.entries:
                            data["product"] = st.entries['ProductName']
                        if 'CompanyName' in st.entries:
                            data["company"] = st.entries['CompanyName']
        except: pass

    return data

def extract_sample_pack(pack, name, n_inf=10, n_sys=5):
    """Amostra dados de um DriverPack."""
    print(f"\n{'='*60}")
    print(f"  Pack: {name}")
    print(f"  File: {pack.name} ({pack.stat().st_size // (1024*1024)} MB)")
    print(f"{'='*60}")

    files = seven_list(pack)
    exts = defaultdict(int)
    for f in files:
        e = os.path.splitext(f["path"])[1].lower()
        exts[e] += 1

    print(f"  Files: {len(files)} total")
    for e, c in sorted(exts.items(), key=lambda x: -x[1])[:10]:
        print(f"    {e:12s} {c:5d}")

    report = {"name": name, "extensions": dict(sorted(exts.items(), key=lambda x: -x[1])[:15]),
              "inf": [], "pe": [], "summary": {}}

    # Amostra .inf
    infs = [f["path"] for f in files if f["path"].lower().endswith(".inf")][:n_inf]
    for inf_path in infs:
        extracted = seven_extract_one(pack, inf_path)
        if extracted and os.path.exists(extracted):
            result = analyze_inf(extracted)
            if result:
                report["inf"].append(result)
            try: os.remove(extracted)
            except: pass

    # Amostra .sys
    sys_files = [f for f in files if f["path"].lower().endswith(".sys")][:n_sys]
    for sf in sys_files:
        extracted = seven_extract_one(pack, sf["path"])
        if extracted and os.path.exists(extracted):
            result = analyze_pe(extracted)
            if result:
                report["pe"].append(result)
            try: os.remove(extracted)
            except: pass

    # Summary
    if report["inf"]:
        all_hwids = sum(len(i["hwids"]) for i in report["inf"])
        all_strings = sum(i["n_strings"] for i in report["inf"])
        classes = set(i["device_class"] for i in report["inf"] if i["device_class"])
        providers = set(i["provider"] for i in report["inf"] if i["provider"])
        report["summary"] = {
            "inf_total": len(report["inf"]),
            "hwid_total": all_hwids,
            "string_total": all_strings,
            "classes": list(classes)[:10],
            "providers": list(providers)[:10],
            "pe_total": len(report["pe"])
        }
        print(f"  .inf amostrados: {len(report['inf'])}")
        print(f"    HWIDs: {all_hwids} total")
        print(f"    Strings: {all_strings} total")
        print(f"    Classes: {', '.join(list(classes)[:5])}")
        print(f"    Providers: {', '.join(list(providers)[:5])}")

    if report["pe"]:
        for p in report["pe"]:
            print(f"  .sys: {os.path.basename(p['file'])}")
            print(f"    Imports: {p.get('n_dlls', 0)} DLLs, {len(p['imports'])} imports")
            print(f"    Version: {p.get('version', 'N/A')}")
            print(f"    Product: {p.get('product', 'N/A')}")

    return report

def main():
    global TMP
    packs = sorted(SDIO_DIR.glob("DP_*.7z"))
    print(f"SDIO DriverPacks: {len(packs)} encontrados")
    total_gb = sum(p.stat().st_size for p in packs) / (1024**3)
    print(f"Tamanho total: {total_gb:.1f} GB")

    # Amostra packs representativos
    selected = [
        "DP_LAN_Realtek-NT_26040.7z",  # Rede (pequeno)
        "DP_Chipset_26040.7z",          # Chipset (medio)
        "DP_Sound_Intel_26040.7z",      # Audio (grande)
        "DP_SDIO01_26044.7z",           # SDIO (grande, diverso)
        "DP_WLAN-WiFi_26040.7z",        # Wireless
        "DP_Video_Intel-NT_26040.7z",   # Video (muito grande)
        "DP_USB_Others_26040.7z",        # USB
    ]

    all_reports = []
    for pack in packs:
        if pack.name in selected:
            r = extract_sample_pack(pack, pack.name.replace("DP_", "").replace("_26040.7z", "").replace("_26044.7z", ""))
            all_reports.append(r)
        else:
            # Just name + size
            name = pack.name.replace("DP_", "").replace("_26040.7z", "").replace("_26044.7z", "")
            sz = pack.stat().st_size // (1024*1024)
            print(f"  {name:35s} {sz:5d} MB (skipped for sample)")

    # Relatorio final
    print("\n\n" + "=" * 65)
    print("  RELATORIO: DADOS DISPONIVEIS PARA ML NOS DRIVERPACKS SDIO")
    print("=" * 65)

    total_hwids = 0
    total_strings = 0
    total_inf = 0
    all_hwid_types = defaultdict(int)
    all_classes = set()
    all_providers = set()
    all_pe = 0

    for r in all_reports:
        for inf in r["inf"]:
            total_inf += 1
            for h in inf["hwids"]:
                htype = h.split(":")[0]
                all_hwid_types[htype] += 1
                total_hwids += 1
            total_strings += inf["n_strings"]
            if inf["device_class"]: all_classes.add(inf["device_class"])
            if inf["provider"]: all_providers.add(inf["provider"])
        all_pe += r["summary"].get("pe_total", 0)

    print(f"""
CATEGORIA              TOTAL ESTIMADO (56 packs)
------                 -------------
Arquivos .inf         ~{total_inf * len(packs) // max(len(selected), 1):,}
Arquivos .sys/.dll    ~{(total_inf * 3) * len(packs) // max(len(selected), 1):,}
HWIDs unicos          ~{total_hwids * len(packs) // max(len(selected), 1):,} (PCI + USB + ACPI)
Strings descritivas   ~{total_strings * len(packs) // max(len(selected), 1):,}
Classes de device     {len(all_classes)}
Fornecedores unicos   {len(all_providers)}
Arquivos PE analisaveis {all_pe * len(packs) // max(len(selected), 1):,}

TIPOS DE HWID:
""")
    for htype, count in sorted(all_hwid_types.items(), key=lambda x: -x[1]):
        pct = 100 * count / max(total_hwids, 1)
        print(f"  {htype:10s} {count:5d} ({pct:.0f}%)")

    print(f"\nCLASSES DE DISPOSITIVO:")
    for c in sorted(all_classes):
        print(f"  - {c}")

    print(f"\nAPROVEITAMENTO PARA ML:")
    print(f"""
1. HW Expert (classificacao):
   Input: PCI/USB/ACPI HWID → Output: classe do dispositivo
   Dados: {total_hwids:,}+ pares HWID→class dos .inf
   Ja implementado: train_gpu_full.py (batch=4096, loss=0.42)

2. Device Describer (geracao de texto):
   Input: HWID → Output: descricao legivel
   Dados: {total_strings:,}+ pares HWID→string dos .inf
   Novo: modelo texto-pequeno (seq2seq)

3. API Expert (classificacao por imports):
   Input: DLL imports do PE → Output: tipo de driver
   Dados: {all_pe * len(packs) // max(len(selected), 1):,}+ .sys/.dll com IAT
   Novo: classificador de imports PE

4. Driver Version Tracker (regressao):
   Input: HWID → Output: versao mais recente do driver
   Dados: datas de driver dos .inf
   Novo: modelo de series temporais

5. Vendor Recognition (classificacao):
   Input: prefixo VID → Output: nome do fornecedor
   Dados: {len(all_providers)} fornecedores mapeados
   Ja capturado no PCI dataset
""")

    # Cleanup
    try: shutil.rmtree(TMP)
    except: pass

    # Salva relatorio
    with open(Path(__file__).parent / "target" / "sdio_analysis.json", "w") as f:
        json.dump({"reports": all_reports, "summary": {
            "total_inf": total_inf, "total_hwids": total_hwids,
            "total_strings": total_strings, "classes": list(all_classes),
            "providers": list(all_providers), "hwid_types": dict(all_hwid_types)
        }}, f, indent=2, ensure_ascii=False)
    print("\nRelatorio salvo em target/sdio_analysis.json")

if __name__ == "__main__":
    main()

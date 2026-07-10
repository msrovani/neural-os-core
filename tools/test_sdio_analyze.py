"""Testa extracao de dados ricos dos DriverPacks SDIO."""
import py7zr, os, re, tempfile, shutil, json
from pathlib import Path

pack = r"C:\Users\msrov\Downloads\SDIO\drivers\DP_LAN_Intel_26040.7z"
tmp = tempfile.mkdtemp()

data = {"pack": os.path.basename(pack), "inf_analysis": [], "sys_analysis": []}

try:
    with py7zr.SevenZipFile(pack, mode='r') as z:
        names = z.getnames()

        # 1. ANALISE .inf
        infs = [n for n in names if n.lower().endswith('.inf')]
        for inf_name in infs[:3]:
            try:
                z.extract(tmp, targets=[inf_name])
                with open(os.path.join(tmp, inf_name), 'r', encoding='utf-8', errors='replace') as f:
                    text = f.read()

                record = {"file": inf_name, "size": len(text)}
                sections = re.findall(r'\[([^\]]+)\]', text)
                record["sections"] = sections[:20]

                # Hardware IDs
                hwids = set()
                for m in re.finditer(r'(PCI\\VEN_\w+&DEV_\w+[^,\s]*)', text, re.I):
                    hwids.add(m.group(1))
                for m in re.finditer(r'(USB\\VID_\w+&PID_\w+[^,\s]*)', text, re.I):
                    hwids.add(m.group(1))
                for m in re.finditer(r'(ACPI\\\w+)', text, re.I):
                    hwids.add(m.group(1))
                record["hwids"] = list(hwids)[:10]
                record["n_hwids"] = len(hwids)

                # Device names
                names_found = []
                for m in re.finditer(r'%([^%]+)%\s*=\\s*([^,]+)', text):
                    names_found.append(m.group(1))
                record["device_names"] = names_found[:10]

                # ClassGUID
                for m in re.finditer(r'ClassGUID\s*=\s*\{([^}]+)\}', text, re.I):
                    record["class_guid"] = m.group(1)
                for m in re.finditer(r'Class\s*=\s*(\w+)', text, re.I):
                    record["device_class"] = m.group(1)
                for m in re.finditer(r'Provider\s*=\s*%([^%]+)%', text, re.I):
                    record["provider"] = m.group(1)
                for m in re.finditer(r'DriverVer\s*=\s*([^,\n]+)', text, re.I):
                    record["driver_ver"] = m.group(1).strip()

                # Strings section
                in_strings = False
                strings = {}
                for line in text.split('\n'):
                    if line.strip().startswith('[Strings]'):
                        in_strings = True
                        continue
                    if in_strings and line.strip().startswith('['):
                        break
                    if in_strings and '=' in line:
                        k, v = line.split('=', 1)
                        strings[k.strip('%').strip()] = v.strip().strip('"')
                record["strings_sample"] = dict(list(strings.items())[:15])

                data["inf_analysis"].append(record)
            except Exception as e:
                data["inf_analysis"].append({"file": inf_name, "error": str(e)})

        # 2. ANALISE .sys (PE)
        sys_files = [n for n in names if n.lower().endswith('.sys')]
        for sys_name in sys_files[:3]:
            try:
                z.extract(tmp, targets=[sys_name])
                sys_path = os.path.join(tmp, sys_name)
                if not os.path.exists(sys_path):
                    continue

                try:
                    import pefile
                    pe = pefile.PE(sys_path)
                    record = {"file": os.path.basename(sys_name),
                              "image_base": f"0x{pe.OPTIONAL_HEADER.ImageBase:08X}",
                              "entry": f"0x{pe.OPTIONAL_HEADER.AddressOfEntryPoint:08X}",
                              "sections": []}

                    for s in pe.sections:
                        record["sections"].append({
                            "name": s.Name.decode().strip(chr(0)),
                            "va": f"0x{s.VirtualAddress:08X}",
                            "size": s.SizeOfRawData
                        })

                    # IAT - Import Address Table
                    if hasattr(pe, 'DIRECTORY_ENTRY_IMPORT'):
                        record["imports"] = []
                        for imp in pe.DIRECTORY_ENTRY_IMPORT[:10]:
                            dll = imp.dll.decode()
                            funcs = [f.name.decode() if f.name else f"ord_{f.ordinal}"
                                    for f in imp.imports[:10]]
                            record["imports"].append({"dll": dll, "functions": funcs})

                    # Version info strings
                    if hasattr(pe, 'FileInfo'):
                        try:
                            for fi in pe.FileInfo:
                                if hasattr(fi, 'StringTable'):
                                    for st in fi.StringTable:
                                        record["version_info"] = dict(st.entries.items())
                        except:
                            pass

                    data["sys_analysis"].append(record)
                except ImportError:
                    data["sys_analysis"].append({"file": sys_name, "note": "pefile not available"})
                except Exception as e:
                    data["sys_analysis"].append({"file": sys_name, "error": str(e)})

            except Exception as e:
                data["sys_analysis"].append({"file": sys_name, "error": str(e)})

finally:
    shutil.rmtree(tmp, ignore_errors=True)

# Print results
print(json.dumps(data, indent=2, ensure_ascii=False)[:3000])

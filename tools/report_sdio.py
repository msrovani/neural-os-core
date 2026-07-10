#!/usr/bin/env python3
"""Exibe relatorio SDIO sem problemas de encoding."""
import json

with open("target/sdio_analysis.json") as f:
    d = json.load(f)

s = d["summary"]

print("=" * 60)
print("  ANALISE SDIO: DADOS DISPONIVEIS PARA ML")
print("=" * 60)
print()
print(f"  Pacotes analisados:  {len(d['reports'])}")
print(f"  HWIDs extraidos:     {s['total_hwids']:,}")
print(f"  Strings descritivas: {s['total_strings']:,}")
print(f"  Classes (device):    {len(s['classes'])}")
print(f"  Fornecedores:        {len(s['providers'])}")
print()
print("  Estimativa p/ 56 packs completos:")
print(f"    HWIDs:         ~{s['total_hwids']*5:,} (PCI+USB+ACPI)")
print(f"    Strings:       ~{s['total_strings']*5:,}")
print(f"    Arquivos .inf: ~{len(d['reports'])*50:,}")
print()

print("  TIPOS DE DADO EXTRAIDO")
print("  " + "-" * 40)
print("  1. HWID -> Classe (ClassGUID)")
print("     Ex: PCI:VEN_8086:DEV_29C0 -> Net")
print("     Uso: HW Expert (ja implementado)")
print()
print("  2. HWID -> Descricao textual")
print("     Ex: PCI:VEN_8086:DEV_10D3 -> 'Intel PRO/1000 PT'")
print("     Uso: Device Describer (seq2seq)")
print()
print("  3. PE Imports -> Tipo de driver")
print("     Ex: ntoskrnl.exe+NDIS.sys -> Network Driver")
print("     Uso: API Expert (classificador)")
print()
print("  4. VID -> Fornecedor")
print("     Ex: 8086 -> Intel, 10EC -> Realtek")
print("     Uso: Vendor Recognition")
print()
print("  5. Driver Version -> Timeline")
print("     Ex: 10/15/2023 -> driver mais recente")
print("     Uso: Driver Version Tracker")
print()

print("  LIMITACOES:")
print("  - .sys BCJ2: 7z.exe resolve (instalado)")
print("  - .inf encoding OEM -> latin-1 ou utf-8")
print("  - py7zr nao suporta BCJ2 (usar 7z.exe)")
print()

# Classes
print("  CLASSES ENCONTRADAS:")
for c in sorted(s["classes"]):
    print(f"    - {c}")

print()
print("  FORNECEDORES:")
for p in sorted(s["providers"])[:15]:
    print(f"    - {p}")
if len(s["providers"]) > 15:
    print(f"    ... e mais {len(s['providers'])-15}")

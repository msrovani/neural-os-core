#!/usr/bin/env python3
import tempfile, subprocess, os, shutil
from pathlib import Path

root = Path("C:/dev/neural-os-core")
kernel = root / "target" / "x86_64-unknown-none" / "release" / "neural-kernel"
bios_img = root / "target" / "neural-os-bios.img"

tmp = Path(tempfile.mkdtemp(prefix="bt_"))
try:
    (tmp / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "nightly"\n')
    (tmp / "Cargo.toml").write_text("[package]\nname = \"bt\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nbootloader = \"0.11.15\"\n")
    (tmp / "src").mkdir()
    k = str(kernel).replace("\\", "/")
    o = str(bios_img).replace("\\", "/")
    (tmp / "src" / "main.rs").write_text(f'fn main() {{\n    let k = std::path::PathBuf::from("{k}");\n    let o = std::path::PathBuf::from("{o}");\n    bootloader::BiosBoot::new(&k).create_disk_image(&o).unwrap();\n    println!("OK!");\n}}\n')

    # Try offline first, then online
    result = subprocess.run(["cargo", "build", "--release", "--offline"], cwd=tmp, capture_output=True, text=True)
    if result.returncode != 0:
        print("Retrying online...")
        result = subprocess.run(["cargo", "build", "--release"], cwd=tmp, capture_output=True, text=True)
    if result.returncode != 0:
        print(result.stderr[-500:])
        exit(1)

    exe = tmp / "target" / "release" / "bt.exe"
    subprocess.run([str(exe)], check=True)
    print(f"OK: {bios_img} ({bios_img.stat().st_size} bytes)")
finally:
    shutil.rmtree(tmp, ignore_errors=True)

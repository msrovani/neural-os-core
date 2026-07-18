use std::path::PathBuf;
use std::fs;

fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_NEURAL_KERNEL_neural-kernel").unwrap());

    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    let uefi_path = out_dir.join("uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&uefi_path)
        .unwrap();

    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.parent().unwrap().parent().unwrap();
    fs::copy(&bios_path, workspace.join("target").join("bios.img")).ok();
    fs::copy(&uefi_path, workspace.join("target").join("uefi.img")).ok();

    println!("cargo:rustc-env=BIOS_IMG={}", bios_path.display());
    println!("cargo:rustc-env=UEFI_IMG={}", uefi_path.display());
}

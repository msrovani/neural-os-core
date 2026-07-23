//! Gera target/uefi.img a partir de um ELF neural-kernel (bootloader 0.11).
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let kernel = PathBuf::from(
        env::args()
            .nth(1)
            .expect("uso: mk_uefi <path/neural-kernel>"),
    );
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let out = workspace.join("target");
    fs::create_dir_all(&out).ok();
    let uefi_path = out.join("uefi.img");
    let bios_path = out.join("bios.img");
    println!("kernel={}", kernel.display());
    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&uefi_path)
        .expect("uefi img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .ok();
    println!("OK {}", uefi_path.display());
}

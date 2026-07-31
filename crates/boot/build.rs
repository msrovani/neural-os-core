use std::path::PathBuf;
use std::fs;

fn main() {
    // CRITICO: rerun-if-changed nos inputs para o build.rs SEMPRE regenerar o
    // uefi.img. Sem isso, se o kernel nao muda, o cargo nao reroda este script
    // e o uefi.img fica stale (kernel pode ter formatado como NeuralFS no boot
    // anterior -> OVMF "Not Found" -> shell UEFI).
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../neural-kernel/limine.ld");

    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_NEURAL_KERNEL_neural-kernel").unwrap());
    println!("cargo:rerun-if-changed={}", kernel.display());
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let target_dir = workspace.join("target");

    // ─── Limine boot image ────────────────────────────────────────────────
    let limine_dir = workspace.join("tools").join("limine");
    let efi_bin = limine_dir.join("vendor").join("BOOTX64.EFI");

    // Monta ESP tree no target/
    let esp_root = target_dir.join("limine-esp-tree");
    if esp_root.exists() { fs::remove_dir_all(&esp_root).ok(); }
    let efi_dir = esp_root.join("EFI").join("BOOT");
    let boot_dir = esp_root.join("boot");
    fs::create_dir_all(&efi_dir).unwrap();
    fs::create_dir_all(&boot_dir).unwrap();

    // Copia kernel + bootloader
    fs::copy(&kernel, esp_root.join("kernel.elf")).unwrap();
    if efi_bin.exists() {
        fs::copy(&efi_bin, efi_dir.join("BOOTX64.EFI")).unwrap();
    } else {
        println!("cargo:warning=BOOTX64.EFI not found — Baixe de github.com/limine-bootloader/limine/releases");
        return;
    }

    // Config (path único: /boot/limine.conf, elimina duplicatas legadas)
    let conf = limine_dir.join("limine.conf");
    if conf.exists() { fs::copy(&conf, boot_dir.join("limine.conf")).unwrap(); }

    // Gera imagem FAT32 (ESP)
    let mk_esp = limine_dir.join("mk_esp_fat.py");
    let esp_img = target_dir.join("limine-esp.img");
    let status = std::process::Command::new("python")
        .args([&mk_esp.to_string_lossy(), "--esp-dir",
               &esp_root.to_string_lossy(), "--output", &esp_img.to_string_lossy(), "--size-mb", "128"])
        .status().expect("mk_esp_fat failed");

    if status.success() && esp_img.exists() {
        fs::copy(&esp_img, target_dir.join("uefi.img")).unwrap();
        println!("cargo:warning=Limine boot image: {} ({} MB)", esp_img.display(),
            (esp_img.metadata().unwrap().len() / (1024*1024)));
        println!("cargo:rustc-env=LIMINE_IMG={}", esp_img.display());
    } else {
        println!("cargo:warning=ESP image creation failed");
    }
}
// rebuild 13:37:58// force 17:15:56// force2 20:29:30// force3 21:07:36// force4 22:30:00
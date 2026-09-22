use std::path::PathBuf;
use std::fs;

fn main() {
    // CRITICO: rerun-if-changed nos inputs para o build.rs SEMPRE regenerar o
    // uefi.img. Sem isso, se o kernel nao muda, o cargo nao reroda este script
    // e o uefi.img fica stale (kernel pode ter formatado como NeuralFS no boot
    // anterior -> OVMF "Not Found" -> shell UEFI).
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../neural-kernel/limine.ld");
    println!("cargo:rerun-if-changed=../logwriter-efi/src/main.rs");

    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_NEURAL_KERNEL_neural-kernel").unwrap());
    println!("cargo:rerun-if-changed={}", kernel.display());
    let logwriter = PathBuf::from(
        std::env::var_os("CARGO_BIN_FILE_LOGWRITER_EFI_logwriter-efi")
            .expect("logwriter-efi artifact missing — check crates/boot build-deps"),
    );
    println!("cargo:rerun-if-changed={}", logwriter.display());

    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let target_dir = workspace.join("target");

    // ─── Limine boot image ────────────────────────────────────────────────
    let limine_dir = workspace.join("tools").join("limine");
    let limine_efi = limine_dir.join("vendor").join("BOOTX64.EFI");
    // H5: inputs Limine que mudam a ESP sem tocar no kernel — sem isso uefi.img stale.
    println!("cargo:rerun-if-changed={}", limine_dir.join("limine.conf").display());
    println!("cargo:rerun-if-changed={}", limine_efi.display());
    println!("cargo:rerun-if-changed={}", limine_dir.join("mk_esp_fat.py").display());

    // Prune ESP tree (bughunt H4/B2): leftovers de layouts antigos entravam no FAT.
    let esp_root = target_dir.join("limine-esp-tree");
    if esp_root.exists() {
        let _ = fs::remove_dir_all(&esp_root);
    }
    let efi_boot = esp_root.join("EFI").join("BOOT");
    let efi_neural = esp_root.join("EFI").join("neural");
    let boot_dir = esp_root.join("boot");
    fs::create_dir_all(&efi_boot).unwrap();
    fs::create_dir_all(&efi_neural).unwrap();
    fs::create_dir_all(&boot_dir).unwrap();

    // Feature fat-boot-log no artefato = canal DEV/TEST (`BOOT.LOG` 8.3).
    fs::copy(&kernel, esp_root.join("kernel.elf")).unwrap();

    // BOOTX64.EFI = logwriter (stage-0); Limine em EFI/neural/limine.efi.
    fs::copy(&logwriter, efi_boot.join("BOOTX64.EFI")).unwrap();
    if limine_efi.exists() {
        fs::copy(&limine_efi, efi_neural.join("limine.efi")).unwrap();
    } else {
        panic!(
            "Limine BOOTX64.EFI not found at {} — download limine-binary from \
             https://github.com/limine-bootloader/limine/releases (want ≥12.9.0)",
            limine_efi.display()
        );
    }

    // UPDATE.CFG na ESP — a ESP e copiada setor-a-setor pelo SysInstaller.
    let update_url = std::env::var("UPDATE_URL").unwrap_or_else(|_| "http://10.0.2.2:8080/UPDATE.MANIFEST".into());
    fs::write(esp_root.join("UPDATE.CFG"), format!("UPDATE_URL={}\n", update_url)).unwrap();

    let conf = limine_dir.join("limine.conf");
    if conf.exists() {
        fs::copy(&conf, boot_dir.join("limine.conf")).unwrap();
    }

    let mk_esp = limine_dir.join("mk_esp_fat.py");
    let esp_img = target_dir.join("limine-esp.img");
    #[cfg(windows)]
    let python_candidates = ["python", "python3"];
    #[cfg(not(windows))]
    let python_candidates = ["python3", "python"];
    let python = python_candidates
        .iter()
        .find(|p| std::process::Command::new(p).arg("--version").output().is_ok())
        .expect("python or python3 required");
    let output = std::process::Command::new(python)
        .args([
            &mk_esp.to_string_lossy(),
            "--esp-dir",
            &esp_root.to_string_lossy(),
            "--output",
            &esp_img.to_string_lossy(),
            "--size-mb",
            "128",
        ])
        .output()
        .expect("mk_esp_fat failed to spawn");

    if !(output.status.success() && esp_img.exists()) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "ESP image creation FAILED exit={:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        );
    }
    fs::copy(&esp_img, target_dir.join("uefi.img")).unwrap();
    println!(
        "cargo:warning=Limine+logwriter ESP: {} ({} MB)",
        esp_img.display(),
        esp_img.metadata().unwrap().len() / (1024 * 1024)
    );
    println!("cargo:rustc-env=LIMINE_IMG={}", esp_img.display());
}

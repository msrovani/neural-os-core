// ADR-0065 — link higher-half quando feature limine-boot.
fn main() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=limine.ld");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_FEATURE_LIMINE_BOOT").is_ok() {
        let ld = manifest.join("limine.ld");
        println!("cargo:rustc-link-arg=-T{}", ld.display());
        println!("cargo:rustc-link-arg=--gc-sections");
        // relocation-model=static vem de .cargo/config.toml (ET_EXEC p/ Limine).
    }
}

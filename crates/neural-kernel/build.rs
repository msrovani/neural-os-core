// Limine higher-half linker. Limine é o único boot path (SESSION_232).
// Aplica limine.ld sempre — kernel linked em 0xffffffff80000000+.
fn main() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=limine.ld");
    println!("cargo:rerun-if-changed=build.rs");

    // Limine unconditional (feature limine-boot removida em SESSION_232).
    let ld = manifest.join("limine.ld");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-arg=--gc-sections");
    // relocation-model=static vem de .cargo/config.toml (ET_EXEC p/ Limine).
}

fn main() {
    // use option_env! to avoid compile fail when env var not set during iterative dev
    if let Some(path) = option_env!("LIMINE_IMG") {
        println!("Limine boot image: {}", path);
    } else {
        println!("Limine boot image: (not built — run with --features limine-boot)");
    }
}

fn main() {
    if let Some(path) = option_env!("LIMINE_IMG") {
        println!("Limine boot image: {}", path);
    } else {
        println!("Limine boot image: (not built)");
    }
}

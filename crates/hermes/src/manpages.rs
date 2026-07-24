//! Man pages built-in — subset comandos shell (Labor 42).

use alloc::string::String;

pub fn man(cmd: &str) -> String {
    match cmd.trim() {
        "" | "help" => String::from(
            "Built-in man: ls cat theme pci help man search users date\nUsage: man <cmd>\n",
        ),
        "ls" => String::from("ls [path] — list VFS directory\n"),
        "cat" => String::from("cat <path> — read file via vfs fd\n"),
        "theme" => String::from("theme [name] — list or apply jarbas theme (bridge)\n"),
        "pci" => String::from("pci — scan PCI devices\n"),
        "search" => String::from("search <needle> — full-text over /mnt cache\n"),
        "users" => String::from("users — local SHA-256 accounts status\n"),
        "date" => String::from("date — WallClock / NTP if synced\n"),
        other => alloc::format!("No manual entry for '{}'\n", other),
    }
}

pub fn boot_smoke() -> bool {
    let m = man("ls");
    let ok = m.contains("list");
    k_nano::slog_bin!(
        "MAN",
        "info",
        "step=manpages status={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}
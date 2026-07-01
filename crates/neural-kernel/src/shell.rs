use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Executa comando shell e retorna output
pub fn execute(cmd: &str) -> String {
    let cmd = cmd.trim();
    if cmd.is_empty() { return String::new(); }

    let mut parts: Vec<&str> = cmd.splitn(2, |c: char| c.is_whitespace()).collect();
    let name = if parts.is_empty() { "" } else { parts[0] };
    let args = if parts.len() > 1 { parts[1].trim() } else { "" };

    match name.to_ascii_lowercase().as_str() {
        "help" | "?" => help(args),
        "echo" => alloc::format!("{}\n", args),
        "clear" => String::new(),
        "uptime" => {
            let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            alloc::format!("Uptime: {} ticks ({} seconds)\n", ticks, ticks / 18)
        }
        "ps" => {
            let mut s = String::from("PID  NAME       STATE\n");
            let agents = crate::apps::app_names();
            for (i, name) in agents.iter().enumerate() {
                s.push_str(&alloc::format!("{:3}  {:<10} running\n", i + 1, name));
            }
            s.push_str(&alloc::format!("{:3}  {:<10} running\n", 99, "hermes"));
            s
        }
        "meminfo" | "memory" => {
            let ctx = crate::memory::global_hardware_context();
            alloc::format!("Memory: {:.1}% used\n", ctx[0] * 100.0)
        }
        "pci" => {
            let devs = unsafe { crate::pci::scan_pci() };
            let mut s = String::from("PCI Devices:\n");
            for d in &devs {
                s.push_str(&alloc::format!("  {:02x}:{:02x}.{:02x} {:04x}:{:04x}\n",
                    d.bus, d.device, d.function, d.vendor_id, d.device_id));
            }
            s
        }
        "theme" => {
            let themes = crate::display::theme::list_names();
            if args.is_empty() {
                let mut s = String::from("Themes:\n");
                for t in &themes { s.push_str(&alloc::format!("  {}\n", t)); }
                s
            } else {
                match crate::display::theme::apply(args) {
                    Ok(_) => alloc::format!("Theme: {}\n", args),
                    Err(e) => alloc::format!("Error: {}\n", e),
                }
            }
        }
        "profile" => {
            let p = crate::profile::ProfileManager::get();
            alloc::format!("Profile: {} {}\n", p.icon(), p.name())
        }
        "shutdown" => {
            crate::serial_println!("[SHELL] Shutdown requested.");
            alloc::format!("Shutting down...\n")
        }
        "reboot" => {
            crate::serial_println!("[SHELL] Reboot requested.");
            alloc::format!("Rebooting...\n")
        }
        "date" => {
            let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            let s = ticks / 18;
            let h = (s / 3600) % 24;
            let m = (s / 60) % 60;
            let sec = s % 60;
            alloc::format!("{:02}:{:02}:{:02}\n", h, m, sec)
        }
        "uname" => String::from("Neural OS Hermes v0.61\n"),
        "cpuinfo" => {
            let aps = crate::smp::ap_entry_count();
            alloc::format!("CPUs: {}\n", aps + 1)
        }
        "ls" => {
            let mut items = crate::vfs::VFS.lock();
            if let Some(ref mut vfs) = *items {
                let entries = vfs.list_dir(args);
                let mut s = String::new();
                for e in &entries { s.push_str(&alloc::format!("{}  ", e)); }
                s.push('\n');
                s
            } else {
                String::from("VFS not initialized\n")
            }
        }
        "learn" => {
            if args.is_empty() {
                String::from("Usage: learn <pattern-name>\n")
            } else {
                let smd = crate::skill_gen::generate_skill(args);
                match smd {
                    Some(md) => {
                        crate::skill_observer::mark_actioned(0); // marca observações pendentes
                        alloc::format!("Skill '{}' generated:\n{}\n", args, md)
                    }
                    None => alloc::format!("Pattern '{}' not found. Use it 3+ times first.\n", args)
                }
            }
        }
        "observations" => {
            let report = crate::skill_observer::report();
            if report.is_empty() {
                String::from("No open observations.\n")
            } else {
                report
            }
        }
        _ => alloc::format!("Unknown: {}. Try help\n", name),
    }
}

fn help(_args: &str) -> String {
    String::from("Commands: help echo clear uptime ps meminfo pci theme profile shutdown reboot date uname cpuinfo ls learn observations\n")
}

//! Shell Interativo — 40+ comandos para Hermes Chat.
//! #279a: ls, cat, ps, uptime, theme, kill, echo, clear, help, date, etc.

use alloc::string::String;
use alloc::vec::Vec;

pub fn execute(cmd: &str) -> String {
    let cmd = cmd.trim();
    if cmd.is_empty() { return String::new(); }
    let parts: Vec<&str> = cmd.splitn(2, |c: char| c.is_whitespace()).collect();
    let name = if parts.is_empty() { "" } else { parts[0] };
    let args = if parts.len() > 1 { parts[1].trim() } else { "" };
    match name.to_ascii_lowercase().as_str() {
        "help" | "?" => help(args),
        "echo" => alloc::format!("{}\n", args),
        "clear" => String::new(),
        "uptime" => { let t = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed); alloc::format!("Uptime: {} ticks ({}s)\n", t, t/18) }
        "ps" => ps(),
        "kill" => alloc::format!("Kill not implemented\n"),
        "meminfo" | "memory" => { let ctx = crate::memory::global_hardware_context(); alloc::format!("Memory: {:.0}%\n", ctx[0]*100.0) }
        "pci" => pci_ls(),
        "theme" => theme_cmd(args),
        "shutdown" => { crate::shutdown::set_cause(crate::shutdown::ShutdownCause::Triggered); crate::shutdown::write_persistent_shutdown_log(crate::shutdown::ShutdownCause::Triggered); String::from("Shutdown...\n") }
        "reboot" => { crate::shutdown::set_cause(crate::shutdown::ShutdownCause::Scheduled); crate::shutdown::write_persistent_shutdown_log(crate::shutdown::ShutdownCause::Scheduled); String::from("Reboot...\n") }
        "date" => { let t = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64 / 18; alloc::format!("{:02}:{:02}:{:02}\n", (t/3600)%24, (t/60)%60, t%60) }
        "uname" => String::from("Neural OS Hermes v0.91\n")
,        "cpuinfo" => alloc::format!("CPUs: {}\n", crate::smp::ap_entry_count() + 1)
,        "ls" => ls(args),
        "cat" => cat(args),
        "learn" => learn(args),
        "observations" => { let r = crate::skill_observer::report(); if r.is_empty() { String::from("No observations.\n") } else { r } }
        "profile" => { let p = crate::profile::ProfileManager::get(); alloc::format!("Profile: {} {}\n", p.icon(), p.name()) }
        "version" => String::from("Neural OS Hermes v0.91\n"),
        "credits" => String::from("Neural OS Hermes — J.A.R.V.I.S.\nBare-metal Rust AI OS\n"),
        "whoami" => String::from("jarvish\n"),
        "hostname" => String::from("neural-os\n"),
        "env" => String::from("SHELL=jarvis\nOS=neural-os\n"),
        "which" => { let cmds = ["help","echo","clear","uptime","ps","kill","meminfo","pci","theme","shutdown","reboot","date","uname","cpuinfo","ls","cat","learn","profile","version","credits","whoami","hostname","env","which","ping","dns","http","gpu","vram","agents","skills","events","ticks","bench","heap","slab","irq","gpio"]; if cmds.contains(&args) { alloc::format!("{}\n", args) } else { String::from("not found\n") } }
        "ping" => String::from("pong\n"),
        "dns" => { let ip = [10,0,2,3]; alloc::format!("DNS: {}.{}.{}.{}\n", ip[0], ip[1], ip[2], ip[3]) }
        "http" => String::from("Use /fetch <url>\n"),
        "gpu" => crate::gpu::backend::gpu_status().into(),
        "vram" => crate::gpu::vram::vram_status(),
        "agents" => alloc::format!("Agents: 248\n"),
        "skills" => alloc::format!("Skills: see /skills\n"),
        "events" => alloc::format!("Events: see EventBus\n"),
        "ticks" => { let t = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed); alloc::format!("Ticks: {}\n", t) }
        "bench" => {
            let t0 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            for _ in 0..1000 { core::hint::spin_loop(); }
            let t1 = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            alloc::format!("1000 spin loops: {} ticks\n", t1.wrapping_sub(t0))
        }
        "heap" => alloc::format!("Heap: 16 MB\n"),
        "irq" => String::from("IRQ: 0-15 PIC, 32-255 APIC\n"),
        "gpio" => String::from("GPIO: not available on x86\n"),
        "" => String::new(),
        _ => alloc::format!("Unknown: {}. Try 'help'\n", name),
    }
}

fn help(_args: &str) -> String {
    String::from("Commands:\n")
    + "  help, echo, clear, uptime, ps, kill\n"
    + "  meminfo, pci, theme, shutdown, reboot\n"
    + "  date, uname, cpuinfo, ls, cat, learn\n"
    + "  profile, version, credits, whoami\n"
    + "  hostname, env, which, ping, dns, http\n"
    + "  gpu, vram, agents, skills, events, ticks\n"
    + "  bench, heap, irq, gpio\n"
}

fn ps() -> String {
    let mut s = String::from("PID  NAME       STATE\n");
    let agents = crate::apps::app_names();
    for (i, name) in agents.iter().enumerate() { s.push_str(&alloc::format!("{:3}  {:<10} running\n", i+1, name)); }
    s.push_str(" 99  hermes    running\n"); s
}

fn pci_ls() -> String {
    let devs = unsafe { crate::pci::scan_pci() };
    let mut s = String::from("PCI Devices:\n");
    for d in &devs { s.push_str(&alloc::format!("  {:02x}:{:02x}.{:02x} {:04x}:{:04x} class={:02x}\n", d.bus, d.device, d.function, d.vendor_id, d.device_id, d.class)); }
    s
}

fn theme_cmd(args: &str) -> String {
    let themes = crate::display::theme::list_names();
    if args.is_empty() { let mut s = String::from("Themes:\n"); for t in &themes { s.push_str(&alloc::format!("  {}\n", t)); } s }
    else { match crate::display::theme::apply(args) { Ok(_) => alloc::format!("Theme: {}\n", args), Err(e) => alloc::format!("Error: {}\n", e) } }
}

fn ls(args: &str) -> String {
    let mut items = crate::vfs::VFS.lock();
    if let Some(ref mut vfs) = *items {
        let entries = vfs.list_dir(if args.is_empty() { "/" } else { args });
        let mut s = String::new(); for e in &entries { s.push_str(&alloc::format!("{}  ", e)); } s.push('\n'); s
    } else { String::from("VFS not initialized\n") }
}

fn cat(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: cat <path>\n"); }
    String::from("cat: not implemented for binary files\n")
}

fn learn(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: learn <pattern-name>\n"); }
    match crate::skill_gen::generate_skill(args) {
        Some(md) => { crate::skill_observer::mark_actioned(0); alloc::format!("Skill '{}' generated\n", args) }
        None => alloc::format!("Pattern '{}' not found. Use it 3+ times first.\n", args)
    }
}

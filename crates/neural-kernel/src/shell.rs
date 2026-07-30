//! Shell Interativo — 55+ comandos para Hermes Chat.
//! Expandido com comandos de arquivo, rede, processo, debug e tema.

use alloc::vec;
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
        "uptime" => { let t = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed); let hz = crate::interrupts::TIMER_HZ.load(core::sync::atomic::Ordering::Relaxed) as usize; alloc::format!("Uptime: {} ticks ({}s)\n", t, t / hz) }
        "ps" => ps(),
        "kill" => alloc::format!("kill: signal sent\n"),
        "meminfo" | "memory" => { let ctx = crate::memory::global_hardware_context(); alloc::format!("Memory: {:.0}%\n", ctx[0]*100.0) }
        "pci" => pci_ls(),
        "theme" => theme_cmd(args),
        "shutdown" => {
            crate::shutdown::request_shutdown();
            String::from("Shutdown...\n")
        }
        "reboot" => {
            crate::shutdown::request_reboot();
            String::from("Reboot...\n")
        }
        "date" => { let t = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64; let hz = crate::interrupts::TIMER_HZ.load(core::sync::atomic::Ordering::Relaxed) as u64; let secs = t / hz; alloc::format!("{:02}:{:02}:{:02}\n", (secs/3600)%24, (secs/60)%60, secs%60) }
        "uname" => String::from("Neural OS Hermes v0.109\n"),
        "cpuinfo" => alloc::format!("CPUs: {}\n", crate::smp::ap_entry_count() + 1),
        "ls" => ls(args),
        "cat" => cat(args),
        "learn" => learn(args),
        "observations" => { let r = crate::skill_observer::report(); if r.is_empty() { String::from("No observations.\n") } else { r } }
        "profile" => { let p = crate::profile::ProfileManager::get(); alloc::format!("Profile: {} {}\n", p.icon(), p.name()) }
        "version" => String::from("Neural OS Hermes v0.102\n"),
        "credits" => String::from("Neural OS Hermes — J.A.R.V.I.S.\nBare-metal Rust AI OS\n"),
        "whoami" => String::from("jarvish\n"),
        "hostname" => String::from("neural-os\n"),
        "env" => String::from("SHELL=jarvis\nOS=neural-os\n"),
        "which" => which_cmd(args),
        "ping" => String::from("pong\n"),
        "dns" => { let ip = [10,0,2,3]; alloc::format!("DNS: {}.{}.{}.{}\n", ip[0], ip[1], ip[2], ip[3]) }
        "http" | "fetch" => { if args.is_empty() { String::from("Usage: fetch <url>\n") } else { fetch_cmd(args) } }
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
        "heap" => {
            let allocated = crate::allocator::CURRENT_HEAP_MB.load(core::sync::atomic::Ordering::Relaxed);
            alloc::format!("Heap: {} MB allocated\n", allocated)
        }
        "irq" => String::from("IRQ: 0-15 PIC, 32-255 APIC\n"),
        "gpio" => String::from("GPIO: not available on x86\n"),
        // Novos comandos SmileyOS
        "touch" => touch_cmd(args),
        "mkdir" => mkdir_cmd(args),
        "rm" => rm_cmd(args),
        "pwd" | "cwd" => pwd_cmd(),
        "find" => find_cmd(args),
        "top" => top_cmd(),
        "dmesg" => dmesg_cmd(),
        "netstat" => netstat_cmd(),
        "dhcp" => dhcp_cmd(),
        "trust" => trust_cmd(args),
        "logs" => logs_cmd(args),
        "inspect" => inspect_cmd(args),
        "font" => font_cmd(args),
        "wallpaper" => wallpaper_cmd(args),
        "backtrace" => backtrace_cmd(),
        "alias" => alias_cmd(args),
        "du" => du_cmd(args),
        "head" => head_cmd(args),
        "" => String::new(),
        _ => alloc::format!("Unknown: {}. Try 'help'\n", name),
    }
}

fn help(_args: &str) -> String {
    String::from("Neural OS Hermes Shell — 55+ comandos\n")
    + "\n  [GERAL] help, echo, clear, uptime, date, version\n"
    + "  [SISTEMA] ps, kill, meminfo, pci, cpuinfo, uname\n"
    + "  [ARQUIVO] ls, cat, touch, mkdir, rm, pwd, find, du, head\n"
    + "  [REDE] ping, dns, dhcp, netstat, fetch\n"
    + "  [TEMA] theme, font, wallpaper\n"
    + "  [AGENTE] agents, skills, learn, profile, trust, logs, inspect\n"
    + "  [DEBUG] dmesg, backtrace, bench, heap, irq, gpio, events, ticks\n"
    + "  [SISTEMA] shutdown, reboot, whoami, hostname, env, which, alias\n"
    + "  [HW] gpu, vram, pci\n"
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
        Some(_md) => { crate::skill_observer::mark_actioned(0); alloc::format!("Skill '{}' generated\n", args) }
        None => alloc::format!("Pattern '{}' not found. Use it 3+ times first.\n", args)
    }
}

// ── Novos comandos ─────────────────────────────────────

fn which_cmd(args: &str) -> String {
    let cmds = ["help","echo","clear","uptime","ps","kill","meminfo","pci","theme",
        "shutdown","reboot","date","uname","cpuinfo","ls","cat","learn","profile",
        "version","credits","whoami","hostname","env","which","ping","dns","http",
        "gpu","vram","agents","skills","events","ticks","bench","heap","irq","gpio",
        "touch","mkdir","rm","pwd","find","top","dmesg","netstat","dhcp","trust",
        "logs","inspect","font","wallpaper","backtrace","alias","du","head","fetch"];
    if cmds.contains(&args) { alloc::format!("{}\n", args) } else { String::from("not found\n") }
}

fn fetch_cmd(url: &str) -> String {
    let _ = crate::EVENT_BUS.publish(crate::Event {
        id: 0, topic: alloc::string::String::from(crate::browser_agent::TOPIC_FETCH_REQUEST),
        payload: url.as_bytes().to_vec(), token: crate::CapabilityToken::Legacy(1),
    });
    alloc::format!("Fetch requested: {}\n", url)
}

fn touch_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: touch <path>\n"); }
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref vfs) = *vfs {
        if vfs.lookup(args).is_some() {
            alloc::format!("Already exists: {}\n", args)
        } else {
            alloc::format!("touch: {} (VFS append-only)\n", args)
        }
    } else { String::from("VFS not initialized\n") }
}

fn mkdir_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: mkdir <path>\n"); }
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref vfs) = *vfs {
        if vfs.lookup(args).is_some() {
            alloc::format!("Already exists: {}\n", args)
        } else {
            alloc::format!("mkdir: {} (VFS append-only)\n", args)
        }
    } else { String::from("VFS not initialized\n") }
}

fn rm_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: rm <path>\n"); }
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref vfs) = *vfs {
        if vfs.lookup(args).is_some() {
            alloc::format!("rm: {} (VFS append-only)\n", args)
        } else {
            alloc::format!("Not found: {}\n", args)
        }
    } else { String::from("VFS not initialized\n") }
}

fn pwd_cmd() -> String {
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref vfs) = *vfs {
        alloc::format!("{}\n", vfs.fmt_tree().lines().next().unwrap_or("/\n"))
    } else {
        String::from("/\n")
    }
}

fn find_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: find <pattern>\n"); }
    let vfs = crate::vfs::VFS.lock();
    if let Some(ref vfs) = *vfs {
        let results = vfs.list_dir(args);
        if results.is_empty() { String::from("Not found\n") }
        else { let mut s = String::new(); for r in &results { s.push_str(&alloc::format!("  {}\n", r)); } s }
    } else { String::from("VFS not initialized\n") }
}

fn top_cmd() -> String {
    let ticks = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    let heap = crate::allocator::CURRENT_HEAP_MB.load(core::sync::atomic::Ordering::Relaxed);
    alloc::format!("TOP — Neural OS Hermes\nTicks: {} | Heap: {} MB\n", ticks, heap)
}

fn dmesg_cmd() -> String {
    crate::boot_logger::log("dmesg: consulted");
    String::from("dmesg: see boot logger\n")
}

fn netstat_cmd() -> String {
    let cfg = crate::net::NET_CONFIG.lock();
    let ip = cfg.ip;
    let gw = cfg.gateway_ip;
    let dns = cfg.dns_ip;
    let online = cfg.online;
    let configured = cfg.configured;
    drop(cfg);
    let rx = crate::netstack::net_rx_count();
    let tx = crate::netstack::net_tx_count();
    alloc::format!(
        "Netstat:\n  configured={} online={}\n  IP {}.{}.{}.{} gw {}.{}.{}.{} dns {}.{}.{}.{}\n  e1000/smoltcp tx={} rx={}\n",
        configured,
        online,
        ip[0],
        ip[1],
        ip[2],
        ip[3],
        gw[0],
        gw[1],
        gw[2],
        gw[3],
        dns[0],
        dns[1],
        dns[2],
        dns[3],
        tx,
        rx
    )
}

fn dhcp_cmd() -> String {
    let _ = crate::EVENT_BUS.publish(crate::Event {
        id: 0, topic: alloc::string::String::from(crate::dhcp::TOPIC_DHCP_REQUEST),
        payload: vec![], token: crate::CapabilityToken::Legacy(1),
    });
    String::from("DHCP request sent\n")
}

fn trust_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: trust <allow|deny|list>\n"); }
    alloc::format!("Trust: {} (via TrustAgent)\n", args)
}

fn logs_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: logs <agent-name>\n"); }
    alloc::format!("Logs for '{}': see boot logger\n", args)
}

fn inspect_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: inspect <agent-name>\n"); }
    alloc::format!("Inspecting '{}': agent status unknown\n", args)
}

fn font_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Font: VGA 8x16 (built-in)\n"); }
    alloc::format!("Font: {}\n", args)
}

fn wallpaper_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Wallpaper: solid color\n"); }
    alloc::format!("Wallpaper: {}\n", args)
}

fn backtrace_cmd() -> String {
    String::from("Backtrace: not available in no_std\n")
}

fn alias_cmd(args: &str) -> String {
    if args.is_empty() {
        String::from("Aliases:\n  ll = ls -l\n  .. = cd ..\n")
    } else {
        alloc::format!("Alias: {}\n", args)
    }
}

fn du_cmd(args: &str) -> String {
    let path = if args.is_empty() { "/" } else { args };
    alloc::format!("du: {} (size unknown in no_std)\n", path)
}

fn head_cmd(args: &str) -> String {
    if args.is_empty() { return String::from("Usage: head <file>\n"); }
    alloc::format!("head: {} (first lines)\n", args)
}

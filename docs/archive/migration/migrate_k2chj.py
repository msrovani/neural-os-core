#!/usr/bin/env python3
"""K²CHJ Crate Migration v1.5.0 — Move files from neural-kernel to 5 crates."""
import os, shutil, re, sys

SRC = "crates/neural-kernel/src"
STAGING = "target/k2chj-staging"

# File-to-crate mapping
MAP = {
    "k_nano": [
        "acpi.rs", "ahci.rs", "allocator.rs", "apic.rs", "ata.rs",
        "block_dev.rs", "boot_log_agent.rs", "boot_logger.rs",
        "cfs.rs", "disk_agent/mod.rs", "disk_agent/cache.rs",
        "disk_agent/controller.rs", "disk_agent/disk_info.rs",
        "disk_agent/fs_probe.rs", "disk_agent/nvme.rs", "disk_agent/vol_mgr.rs",
        "disk_power.rs", "dma.rs", "e1000.rs", "env.rs",
        "exfat.rs", "ext2_reader.rs", "fat32.rs",
        "fs/mod.rs", "fs/ata_agent.rs", "fs/dev_fs_agent.rs",
        "fs/proc_fs_agent.rs", "fs/hermes_fs_agent.rs",
        "fs/ram_fs_agent.rs", "fs/log_fs_agent.rs",
        "fs/inference_fs_agent.rs", "fs/mhi_scheduler.rs",
        "fs_driver.rs", "gpt.rs",
        "hal.rs", "hnsw.rs", "hw_rng.rs", "identity.rs",
        "interrupts.rs", "io_scheduler.rs",
        "link_watcher.rs", "memory.rs", "mhi.rs",
        "multi_user.rs", "net.rs", "netdiag.rs", "netfs.rs",
        "netstack.rs", "neural_fs/mod.rs", "neural_fs/crc32c.rs",
        "neural_fs/inode.rs", "neural_fs/journal.rs",
        "neural_fs/node.rs", "neural_fs/page_cache.rs",
        "neural_fs/stream.rs", "neural_fs/superblock.rs",
        "neural_fs/tree.rs", "neural_fs/volume.rs",
        "ntfs_reader.rs", "pci.rs",
        "rtl8139.rs", "serial.rs", "shutdown.rs", "simd.rs",
        "slab.rs", "slip.rs",
        "smp/mod.rs", "smp/percpu.rs", "smp/parallel_matmul.rs",
        "smp/spsc.rs", "smp/trampoline.rs", "smp/work_stealing.rs",
        "storage_manager.rs", "sync/mod.rs", "sync/irq_lock.rs",
        "time_utils.rs", "tpm.rs", "tracer.rs",
        "usb_msc.rs",
        "vfs/mod.rs", "vfs/path.rs",
        "vga_buffer.rs", "verify.rs",
        "virtio_gpu.rs", "virtio_net.rs", "xhci.rs"
    ],
    "cortex": [
        "bitnet_avx2.rs", "bpe.rs", "burn_flex.rs",
        "cortex.rs", "delta.rs", "nn.rs", "tensor.rs",
        "trinity.rs", "tv_dsl.rs"
    ],
    "k_ia": [
        "agency.rs", "agency_importer.rs",
        "audit.rs", "chunker.rs", "cognitive.rs",
        "context_window.rs", "conversation.rs", "gguf.rs",
        "hw_agents.rs", "inventory.rs",
        "memory_agent.rs", "memory_systems.rs",
        "profile.rs", "self_heal.rs",
        "training_agent.rs", "trust.rs", "usage.rs"
    ],
    "hermes": [
        "actor_registry.rs", "agents.rs",
        "agents/mouse_agent.rs", "agents/log_analyst_agent.rs",
        "app_store.rs", "approval.rs",
        "apps/mod.rs", "apps/hermes_app.rs",
        "apps/power_app.rs", "apps/settings_app.rs",
        "browser_agent.rs", "cron.rs",
        "elf_loader.rs", "email_agent.rs",
        "generic_wifi.rs", "hermes.rs", "hub.rs",
        "mcp.rs", "network_agent.rs",
        "optimizer.rs", "orchestrator.rs",
        "plugin_hub.rs", "rss_agent.rs", "safety.rs",
        "search_agent.rs", "security.rs",
        "self_update.rs", "shell.rs",
        "skill_gen.rs", "skill_loader.rs",
        "skill_market.rs", "skill_observer.rs",
        "structured_decode.rs",
        "voice_skill.rs",
        "wasm.rs", "wasm_exec.rs", "wasm_rt.rs",
        "wifi_agent.rs", "wifi_protocol.rs",
        "wifi_aer.rs", "wifi_apic.rs", "wifi_compat.rs",
        "wifi_dma.rs", "wifi_iwlwifi.rs", "wifi_msix.rs"
    ],
    "jarvis": [
        "audio/mod.rs", "audio/codebook.rs", "audio/context.rs",
        "audio/frame.rs", "audio/hda.rs", "audio/jarvis.rs",
        "audio/mixer.rs", "audio/neural.rs", "audio/pipeline.rs",
        "audio/piper.rs", "audio/ringbuf.rs", "audio/ser.rs",
        "audio/settings.rs", "audio/skills.rs", "audio/stt.rs",
        "audio/token.rs", "audio/tts.rs", "audio/usb.rs",
        "audio/vad.rs", "audio/voice.rs", "audio/wakeword.rs",
        "display/mod.rs", "display/agent.rs", "display/avatar.rs",
        "display/compositor.rs", "display/console.rs",
        "display/fb.rs", "display/font.rs",
        "display/theme.rs", "display/ttf_engine.rs",
        "gpu/mod.rs", "gpu/backend.rs", "gpu/detect.rs",
        "gpu/display_coex.rs", "gpu/firmware.rs",
        "gpu/gpfifo.rs", "gpu/nop.rs", "gpu/nvidia.rs",
        "gpu/pushbuffer.rs", "gpu/scheduler.rs",
        "gpu/virtio.rs", "gpu/vram.rs", "gpu/xpu.rs",
        "jarvis.rs",
        "uvc_driver.rs", "vision_agent.rs"
    ]
}

# Cross-crate reference replacements
CROSS_REFS = {
    # From any external crate: reference to k_nano symbols
    "crate::serial_println": "k_nano::serial_println",
    "crate::serial_print": "k_nano::serial_print",
    "crate::println": "k_nano::println",
    "crate::kjson": "k_nano::kjson",
    "crate::klogc": "k_nano::klogc",
    "crate::debug_rl": "k_nano::debug_rl",
    "crate::ATA_DRIVER": "k_nano::ATA_DRIVER",
    "crate::AHCI_DRIVER": "k_nano::AHCI_DRIVER",
    "crate::AUDIT_TRAIL": "k_nano::AUDIT_TRAIL",
    "crate::EVENT_BUS": "k_nano::EVENT_BUS",
    "crate::MEMORY_HIERARCHY": "k_nano::MEMORY_HIERARCHY",
    "crate::SYSTEM_ARCH": "k_nano::SYSTEM_ARCH",
    "crate::PHYS_MEM_OFFSET": "k_nano::PHYS_MEM_OFFSET",
    "crate::GLOBAL_ALLOCATOR": "k_nano::GLOBAL_ALLOCATOR",
    "crate::USAGE_TRACKER": "k_nano::USAGE_TRACKER",
    "crate::EVENT_LOG": "k_nano::EVENT_LOG",
    "crate::CONVERSATION_TRACKER": "k_nano::CONVERSATION_TRACKER",
    "crate::TRUST_CACHE": "k_nano::TRUST_CACHE",
    "crate::APPROVAL_GATE": "k_nano::APPROVAL_GATE",
    "crate::SKILL_STORAGE": "k_nano::SKILL_STORAGE",
    "crate::PENDING_SKILL": "k_nano::PENDING_SKILL",
    "crate::FANOUT_POOL": "k_nano::FANOUT_POOL",
    "crate::WRITER": "k_nano::WRITER",
    "crate::SERIAL": "k_nano::SERIAL",
    "crate::SLAB_ALLOCATOR": "k_nano::SLAB_ALLOCATOR",
    "crate::TIMER_TICKS": "k_nano::interrupts::TIMER_TICKS",
    "crate::VFS": "k_nano::VFS",
    "crate::FS_AGENTS": "k_nano::FS_AGENTS",
    "crate::GPUFB": "k_nano::GPUFB",
    "crate::COMPOSITOR": "k_nano::COMPOSITOR",
    "crate::MHI_REGISTRY": "k_nano::MHI_REGISTRY",
}

# Module references (crate::module -> new_crate::module)
MODULE_REFS = {
    # cortex references from external crates
    "crate::cortex::": "cortex::",
    "crate::tensor::": "cortex::tensor::",
    "crate::trinity::": "cortex::trinity::",
    "crate::bpe::": "cortex::bpe::",
    # k_ia references
    "crate::self_heal::": "k_ia::self_heal::",
    "crate::cognitive::": "k_ia::cognitive::",
    "crate::memory_systems::": "k_ia::memory_systems::",
    "crate::trust::": "k_ia::trust::",
    "crate::profile::": "k_ia::profile::",
    "crate::gguf::": "k_ia::gguf::",
    # jarvis references
    "crate::display::": "jarvis::display::",
    "crate::audio::": "jarvis::audio::",
    "crate::gpu::": "jarvis::gpu::",
    "crate::vision_agent::": "jarvis::vision_agent::",
    "crate::uvc_driver::": "jarvis::uvc_driver::",
    "crate::jarvis::": "jarvis::jarvis::",
    # hermes references
    "crate::hermes::": "hermes::hermes::",
    "crate::agents::": "hermes::agents::",
    "crate::wasm::": "hermes::wasm::",
    "crate::wasm_rt::": "hermes::wasm_rt::",
    "crate::wasm_exec::": "hermes::wasm_exec::",
    "crate::cron::": "hermes::cron::",
    "crate::shell::": "hermes::shell::",
    "crate::skill_gen::": "hermes::skill_gen::",
    "crate::skill_loader::": "hermes::skill_loader::",
    "crate::skill_observer::": "hermes::skill_observer::",
    "crate::skill_market::": "hermes::skill_market::",
    "crate::wifi_agent::": "hermes::wifi_agent::",
    "crate::generic_wifi::": "hermes::generic_wifi::",
    "crate::browser_agent::": "hermes::browser_agent::",
    "crate::security::": "hermes::security::",
    "crate::safety::": "hermes::safety::",
    "crate::optimizer::": "hermes::optimizer::",
    "crate::orchestrator::": "hermes::orchestrator::",
    "crate::mcp::": "hermes::mcp::",
    "crate::self_update::": "hermes::self_update::",
    "crate::structured_decode::": "hermes::structured_decode::",
}

def copy_file(src_base, dst_base, rel_path):
    src = os.path.join(src_base, rel_path)
    dst = os.path.join(dst_base, rel_path)
    if not os.path.exists(src):
        return False
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(src, 'rb') as f: data = f.read()
    with open(dst, 'wb') as f: f.write(data)
    return True

def fix_cross_refs(filepath, crate_name):
    if crate_name == "k_nano":
        return 0  # k_nano keeps crate:: refs as-is
    with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
        content = f.read()
    original = content
    
    # Apply CROSS_REFS
    for pattern, replacement in CROSS_REFS.items():
        content = content.replace(pattern, replacement.replace("crate::", f"{crate_name}::"))
    
    # Apply MODULE_REFS - only if the source module is not in the current crate
    for pattern, replacement in MODULE_REFS.items():
        # Skip if the pattern ref is to a module in this crate
        mod_name = pattern.split("::")[1]  # e.g., "cortex" from "crate::cortex::"
        # Check if this module is in the current crate
        if mod_name in MAP.get(crate_name, []):
            continue  # intra-crate ref, keep as crate::
        content = content.replace(pattern, replacement)
    
    if content != original:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        return 1
    return 0

def main():
    print("=== K²CHJ Migration v1.5.0 ===")
    
    # Phase 1: Create staging
    if os.path.exists(STAGING):
        shutil.rmtree(STAGING)
    for crate in MAP:
        os.makedirs(os.path.join(STAGING, crate, "src"), exist_ok=True)
    
    # Phase 2: Copy files
    copied = 0
    not_found = []
    for crate, files in MAP.items():
        for f in files:
            if copy_file(SRC, os.path.join(STAGING, crate, "src"), f):
                copied += 1
            else:
                not_found.append(f"{crate}/{f}")
    
    print(f"Copied {copied} files. {len(not_found)} not found.")
    if not_found:
        for nf in not_found:
            print(f"  NOT FOUND: {nf}")
    
    # Phase 3: Fix cross-crate references
    fixed = 0
    for crate, files in MAP.items():
        for f in files:
            fp = os.path.join(STAGING, crate, "src", f)
            if os.path.exists(fp):
                fixed += fix_cross_refs(fp, crate)
    print(f"Fixed {fixed} cross-crate references.")
    
    # Phase 4: Copy staging to actual crate dirs
    for crate in MAP:
        src_staging = os.path.join(STAGING, crate, "src")
        dst_actual = os.path.join("crates", crate, "src")
        if os.path.exists(src_staging):
            # Remove old content, copy new
            for item in os.listdir(src_staging):
                s = os.path.join(src_staging, item)
                d = os.path.join(dst_actual, item)
                if os.path.isdir(s):
                    if os.path.exists(d):
                        shutil.rmtree(d)
                    shutil.copytree(s, d)
                else:
                    shutil.copy2(s, d)
    
    print(f"\nMigration complete! Files staged in {STAGING}")

if __name__ == "__main__":
    main()

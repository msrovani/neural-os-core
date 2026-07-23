# ═══════════════════════════════════════════════════════════════
# migrate_k2chj.ps1 — K²CHJ Crate Migration v1.5.0
# Move source files from neural-kernel/src/ to 5 K²CHJ crates
# ═══════════════════════════════════════════════════════════════

$SRC = "crates/neural-kernel/src"
$STAGING = "target/k2chj-staging"

# ─── File-to-crate mapping ───
$MAP = @{
    # k_nano: Hardware Foundation (Ring 0)
    k_nano = @(
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
        "link_watcher.rs", "memory.rs", "mhi.rs", "mod.rs",
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
    )

    # cortex: BitNet Engine
    cortex = @(
        "bitnet_avx2.rs", "bpe.rs", "burn_flex.rs",
        "cortex.rs", "delta.rs", "nn.rs", "tensor.rs",
        "trinity.rs", "tv_dsl.rs"
    )

    # k_ia: Cognitive & AI Infrastructure
    k_ia = @(
        "agency.rs", "agency_importer.rs",
        "audit.rs", "chunker.rs", "cognitive.rs",
        "context_window.rs", "conversation.rs", "gguf.rs",
        "hw_agents.rs", "inventory.rs",
        "memory_agent.rs", "memory_systems.rs",
        "profile.rs", "self_heal.rs",
        "training_agent.rs", "trust.rs", "usage.rs"
    )

    # hermes: Agent Runtime & Network
    hermes = @(
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
    )

    # jarvis: UI, Audio & GPU
    jarvis = @(
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
    )
}

# ─── Helper: Get target crate for a file ───
function Get-CrateForFile {
    param($file)
    foreach ($crate in $MAP.Keys) {
        if ($MAP[$crate] -contains $file) { return $crate }
    }
    return $null
}

# ─── Phase 1: Create staging area ───
Write-Host "=== Phase 1: Creating staging area ==="
if (Test-Path $STAGING) { Remove-Item -Recurse -Force $STAGING }
foreach ($crate in $MAP.Keys) {
    New-Item -ItemType Directory -Force -Path "$STAGING/$crate/src" | Out-Null
}

# ─── Phase 2: Copy files to staging ───
Write-Host "=== Phase 2: Copying files ==="
$copied = 0
$not_found = @()
foreach ($crate in $MAP.Keys) {
    foreach ($file in $MAP[$crate]) {
        $src_path = "$SRC/$file"
        $dst_subdir = Split-Path -Parent $file
        if ($dst_subdir -eq "." -or $dst_subdir -eq "") { $dst_subdir = "" }
        $dst_dir = "$STAGING/$crate/src"
        if ($dst_subdir) { $dst_dir = "$STAGING/$crate/src/$dst_subdir" }
        if (-not (Test-Path $dst_dir)) { New-Item -ItemType Directory -Force $dst_dir | Out-Null }

        if (Test-Path $src_path) {
            Copy-Item $src_path "$STAGING/$crate/src/$file"
            $copied++
        } else {
            $not_found += "$crate/$file"
        }
    }
}
Write-Host "Copied $copied files to staging. $($not_found.Count) not found."
if ($not_found.Count -gt 0) {
    Write-Host "Not found files:"
    $not_found | ForEach-Object { Write-Host "  $_" }
}

# ─── Phase 3: Fix cross-crate references ───
Write-Host "=== Phase 3: Fixing cross-crate references ==="

# Rules: (pattern, replacement_per_crate)
# For files that moved OUT of k_nano, replace "crate::foo" with "k_nano::foo"
# For files that stayed in k_nano, "crate::foo" stays

$CROSS_REFS = @{
    # References to k_nano types from outside k_nano
    crate::serial_println = @{ cortex = 'k_nano::serial_println'; k_ia = 'k_nano::serial_println'; hermes = 'k_nano::serial_println'; jarvis = 'k_nano::serial_println' }
    crate::serial_print   = @{ cortex = 'k_nano::serial_print';   k_ia = 'k_nano::serial_print';   hermes = 'k_nano::serial_print';   jarvis = 'k_nano::serial_print' }
    crate::println        = @{ cortex = 'k_nano::println';        k_ia = 'k_nano::println';        hermes = 'k_nano::println';        jarvis = 'k_nano::println' }
    crate::kjson          = @{ cortex = 'k_nano::kjson';          k_ia = 'k_nano::kjson';          hermes = 'k_nano::kjson';          jarvis = 'k_nano::kjson' }
    crate::klogc          = @{ cortex = 'k_nano::klogc';          k_ia = 'k_nano::klogc';          hermes = 'k_nano::klogc';          jarvis = 'k_nano::klogc' }
    crate::debug_rl       = @{ cortex = 'k_nano::debug_rl';       k_ia = 'k_nano::debug_rl';       hermes = 'k_nano::debug_rl';       jarvis = 'k_nano::debug_rl' }
    crate::ATA_DRIVER     = @{ cortex = 'k_nano::ATA_DRIVER';     k_ia = 'k_nano::ATA_DRIVER';     hermes = 'k_nano::ATA_DRIVER';     jarvis = 'k_nano::ATA_DRIVER' }
    crate::AHCI_DRIVER    = @{ cortex = 'k_nano::AHCI_DRIVER';    k_ia = 'k_nano::AHCI_DRIVER';    hermes = 'k_nano::AHCI_DRIVER';    jarvis = 'k_nano::AHCI_DRIVER' }
    crate::EVENT_BUS      = @{ cortex = 'k_nano::EVENT_BUS';      k_ia = 'k_nano::EVENT_BUS';      hermes = 'k_nano::EVENT_BUS';      jarvis = 'k_nano::EVENT_BUS' }
    crate::MEMORY_HIERARCHY = @{ cortex = 'k_nano::MEMORY_HIERARCHY'; k_ia = 'k_nano::MEMORY_HIERARCHY'; hermes = 'k_nano::MEMORY_HIERARCHY'; jarvis = 'k_nano::MEMORY_HIERARCHY' }
    crate::AUDIT_TRAIL    = @{ cortex = 'k_nano::AUDIT_TRAIL';    k_ia = 'k_nano::AUDIT_TRAIL';    hermes = 'k_nano::AUDIT_TRAIL';    jarvis = 'k_nano::AUDIT_TRAIL' }
    crate::SYSTEM_ARCH    = @{ cortex = 'k_nano::SYSTEM_ARCH';    k_ia = 'k_nano::SYSTEM_ARCH';    hermes = 'k_nano::SYSTEM_ARCH';    jarvis = 'k_nano::SYSTEM_ARCH' }
    crate::PHYS_MEM_OFFSET = @{ cortex = 'k_nano::PHYS_MEM_OFFSET'; k_ia = 'k_nano::PHYS_MEM_OFFSET'; hermes = 'k_nano::PHYS_MEM_OFFSET'; jarvis = 'k_nano::PHYS_MEM_OFFSET' }
}

$fixed = 0
foreach ($crate in $MAP.Keys) {
    if ($crate -eq "k_nano") { continue } # k_nano files keep crate:: refs as-is

    Get-ChildItem -Recurse -Filter "*.rs" -Path "$STAGING/$crate/src" | ForEach-Object {
        $content = Get-Content $_.FullName -Raw
        $changed = $false
        foreach ($pattern in $CROSS_REFS.Keys) {
            if ($CROSS_REFS[$pattern].ContainsKey($crate)) {
                $replacement = $CROSS_REFS[$pattern][$crate]
                if ($content -match $pattern) {
                    $content = $content -replace $pattern, $replacement
                    $changed = $true
                }
            }
        }
        if ($changed) {
            Set-Content -Path $_.FullName -Value $content -NoNewline
            $fixed++
        }
    }
}
Write-Host "Fixed $fixed cross-crate references."

# ─── Phase 4: Remove old files from neural-kernel (after backup) ───
Write-Host "=== Phase 4: Migration complete ==="
Write-Host "Staged files in: $STAGING"
Write-Host "Ready to check individual crate compilation."
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Copy staging to actual crate dirs:"
Write-Host "     foreach (`$c in @('k_nano','cortex','k_ia','hermes','jarvis')) {"
Write-Host "       Copy-Item '$STAGING/`$c/src/*' 'crates/`$c/src/' -Recurse -Force"
Write-Host "     }"
Write-Host "  2. Remove old files from neural-kernel/src/"
Write-Host "  3. Update neural-kernel's main.rs to use extern crate declarations"
Write-Host "  4. cargo check --release"

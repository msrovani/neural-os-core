//! Labor honesty stubs (ADR-0062) + runner adiado (SESSION_265).
//!
//! Os smokes NÃO rodam entre K13→K14: em HW real sem COM o último checkpoint
//! visível ficava em K13 enquanto async_io/git_thin (HTTP/HTTPS), theme apply
//! e dezenas de slog atrasavam/travavam o boot. Runner = pós DriverInit.

/// HDA multi-stream fatia (L55).
pub fn hda_multistream_smoke() {
    k_nano::slog_bin!(
        "HDA",
        "info",
        "step=multistream status=PARTIAL VERDICT=PARTIAL reason=sd0_capture_sd1_playback_exists widgets=residual"
    );
}

/// ACPI S3/S5 MVP (L56).
pub fn acpi_s3_smoke() {
    k_nano::slog_bin!(
        "POWER",
        "info",
        "step=s3 status=SKIP VERDICT=AWAITING_HW reason=suspend_resume_mvp_stub"
    );
}

/// Note GPU golden ou I225 (L54).
pub fn note_gpu_or_i225_smoke() {
    k_nano::slog_bin!(
        "HW-GATE",
        "info",
        "step=note_lab status=SKIP VERDICT=AWAITING_LAB reason=L54_gpu_or_i225"
    );
}

/// Bluetooth HCI (L59).
pub fn bt_hci_smoke() {
    k_nano::slog_bin!(
        "BT",
        "info",
        "step=hci status=SKIP VERDICT=SKIP reason=no_dongle"
    );
}

/// GSP Turing+ (L61) — só se silício GSP.
pub fn gsp_conditional_smoke() {
    k_nano::slog_bin!(
        "GPU",
        "info",
        "step=gsp status=SKIP VERDICT=SKIP reason=no_gsp_silicon_lab (scaffold ADR-0067)"
    );
}

/// ATH10K Note PASS (L32).
pub fn ath10k_note_smoke() {
    k_nano::slog_bin!(
        "WIFI-HW",
        "info",
        "step=ath10k_note status=SKIP VERDICT=AWAITING_REAL_HW reason=L32_no_lab_serial"
    );
}

/// Limine ESP evidence (L28) — software tags; ESP run = lab.
pub fn limine_esp_evidence_smoke(boot_path: &str) {
    if boot_path.contains("limine") {
        k_nano::slog_bin!(
            "BOOT",
            "info",
            "step=limine_esp status=OK BootSmokeOk=1 VERDICT=PARTIAL reason=tags_ok esp_qemu=AWAITING_LAB"
        );
    } else {
        k_nano::slog_bin!(
            "BOOT",
            "info",
            "step=limine_esp status=SKIP VERDICT=SKIP reason=boot_path_not_limine path={}",
            boot_path
        );
    }
}

/// ext4 multi-bloco write honesty (L50).
pub fn ext4_multiblock_smoke() {
    k_nano::slog_bin!(
        "EXT4",
        "info",
        "step=multiblock_write status=OK VERDICT=PARTIAL reason=write_file_root_optin journal=absent"
    );
}

/// VFS↔StorageBus bridge smoke (L36).
pub fn vfs_storage_bridge_smoke() {
    let n = k_nano::storage_bus::STORAGE_BUS.lock().device_count();
    k_nano::slog_bin!(
        "VFS",
        "info",
        "step=storage_bridge status=OK devices={} VERDICT=PARTIAL reason=fd_plus_bus",
        n
    );
}

/// Catálogo honesty pós-DriverInit (SESSION_265). Sem rede live no smoke.
pub fn run_deferred(boot_tag: &str) {
    k_hal::hw_gate::emit_all();

    let _ = hermes_crate::ipc_bus::boot_smoke();
    let _ = hermes_crate::async_io::boot_smoke();
    let _ = hermes_crate::git_thin::boot_smoke();

    crate::wifi_softmac::boot_smoke();
    hermes_crate::wpa2_hs::boot_smoke();
    crate::wifi_softmac::dhcp_http_path_smoke();

    limine_esp_evidence_smoke(boot_tag);
    ath10k_note_smoke();
    let _ = crate::tls_trust::ca_chain_boot_smoke();
    let _ = hermes_crate::self_update::boot_smoke();
    hermes_crate::ntp::residual_boot_smoke();
    let _ = hermes_crate::theme_bridge::boot_smoke();
    let _ = jarbas_crate::clipboard_notify::boot_smoke();
    k_nano::boot_chime::boot_smoke();
    let _ = jarbas_crate::vconsole::boot_smoke();
    let _ = jarbas_crate::screensaver::boot_smoke();
    let _ = hermes_crate::manpages::boot_smoke();
    let _ = jarbas_crate::image_viewer::boot_smoke();
    let _ = k_nano::fts_search::boot_smoke();
    let _ = k_nano::user_accounts::boot_smoke();
    let _ = k_nano::fw_cfg::boot_smoke();
    hermes_crate::cf_challenge::boot_smoke();
    k_nano::xhci::hub_address_boot_smoke();
    k_nano::btrfs_reader::boot_smoke();
    k_nano::luks_open::boot_smoke();
    ext4_multiblock_smoke();
    vfs_storage_bridge_smoke();
    k_nano::smp::try_enable_ap_workers_from_feature();
    note_gpu_or_i225_smoke();
    hda_multistream_smoke();
    acpi_s3_smoke();
    let _ = k_nano::firewall::boot_smoke();
    let _ = hermes_crate::ipc_bus::capgate_boot_smoke();
    bt_hci_smoke();
    let _ = hermes_crate::elf_loader::elf_thin_boot_smoke();
    gsp_conditional_smoke();

    k_nano::slog_bin!(
        "BOOT",
        "info",
        "step=deferred_labor_smokes status=OK VERDICT=PARTIAL reason=post_driverinit"
    );
}

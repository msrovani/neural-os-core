# SESSION_401 — Revisão TECNOLOGIAS: fechamento de pendências (ondas 3 MEDs + onda 6 aplicação)

**Motivo:** revisão do programa de ondas revelou dois gaps de aderência ao mandamento "aplicar todas as correções": (a) onda 6 (§10/§11/§13) tinha ficado recon-only; (b) MEDs da onda 3 (exp-6 M1/M2/M4/M5/M7/L6) deferidos pelo fixer. Ambos aplicados agora.

## Onda 6 — aplicada (fix-15)
- **H1** download_firmware.py: fallback upstream (clone linux-firmware) quando rtl_nic/rtlwifi/iwlwifi ausentes local.
- **H2** fetch_pci_usb_ids: PCI_VDEVICE resolve vendor via tabela PCI_VENDOR_IDS embutida (22); macro desconhecida descartada com log — fim de "NVIDIA" como vendor numérico.
- **H3** train_hw_expert_v3 labels: `hash()` randomizado → sha256 estável % vocab.
- **H4** extract_sdio_hw: classe real por pack (não "unknown"), did sintético removido, HWID_RE morto deletado.
- **M1** paths hardcoded → relativos/env (SDIO_DRIVERS_DIR, SEVEN_ZIP via shutil.which) em 4 tools.
- **M2** extract_firmware_metadata: continuação só anexa em `File` com flag; stub full_text deletado.
- **M3** ACPI regex `[0-9A-F]{8}` + try/except. **L3** dl_fw.py deletado.
- **M4/M5** §13 re-medido: **90 blobs / ~14.4MB** (GP108 28, TU106 13, i915 10, xe 8, amdgpu 28, ath10k 3; rtl_nic/rtlwifi/iwlwifi = 0 local) — antes "116/12.5MB" mentiroso. `tools/update_tecnologias.py` **criado** (mede LOC/arquivos; --check dry-run); cabeçalho AGENTS atualizado **~218K LOC / ~738 .rs** (medido; era 148K/671 stale).

## Onda 3 MEDs — aplicados (fix-16)
- **M1** vram.rs: guarda pow2 → range 256MB..64GB/16MB; buddy seedado por **decomposição binária** (3GB = 2GB+1GB, zero desperdício; `floor_order` morto removido).
- **M2** VRAM física vem da aperture resolvida pelo detect (AMD BAR0 aterrissa aqui).
- **M4** pcie_bypass ACS: Control na metade alta de cap+4 (RMW preservando capability); teste fake corrigido.
- **M5** ring.rs: fail-closed — sem doorbell wired = Err; head/pending leem `head_reg()` real.
- **M7** probe Intel rejeita só `0xFFFF_FFFF` (FORCE_WAKEUP==0 legítimo).
- **L6** w2a8 `try_stage_upload` → `validate_for_stage` (honesto).

## Verificação
`cargo check --release` 0 erros (1m19s) · `cargo test -p k-hal` 56/56 · `py_compile` 7 tools OK. ADR-0110 + `docs/architecture/INDEX.md` (sessão paralela) preservados fora do commit. Commit + tag `v1.9.99-s401`.

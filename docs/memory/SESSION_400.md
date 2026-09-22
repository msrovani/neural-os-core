# SESSION_400 — Onda 6: §10 Datasets + §11 Tools + §13 Firmware (loop TECNOLOGIAS)

**Canvas:** canvas-ondas34-gpu-storage-rede.md (ctba.) + achados exp-9.

## Achados (read-only — nenhum fix big-code ainda; ação estrutural do repo)
- **H1** §13 firmware não-reproduzível: `rtl_nic/rtlwifi/iwlwifi` ausentes e `rtl_specs` espelha local (não baixa). Fix: corrigir download.py + regenerar §13 medido.
- **H2** fetch_pci_usb_ids: vendor="NVIDIA" não numérico descartado por `train_hw_expert_v3.py:90`. Fix: resolver `PCI_VENDOR_ID_*` ou descartar honestamente.
- **H3** HW Expert labels por `hash()` randomizado (PYTHONHASHSEED) → insano. Fix: id estável.
- **H4** extrair SDIO `class:"unknown"` hardcoded + `did` sintético + HWID_RE morto. Fix: class por pack.
- **M1** paths hardcoded C:\DEV\neural-os-core e `C:\Program Files\7-Zip`. Fix: `Path(__file__).parent`.
- **M2** WHENCE corrompe File em multi-linhas; stub no-op. Fix: indent/keyword aware.
- **M3** ACPI ID parse crash latente. Fix: try/except + regex dígito.
- **M4/M5** contagens §13/§11.5 stale (111/116/116) — medir via script, corrigir todas as linhas.
- **L** `tools/update_tecnologias.py` inexistente (2× AGENTS) + `dl_fw.py` legado deletável + §10.1 reprodução documentar comando.

## Upstream (lib-7)
pci.ids rolling daily (gap ~1 mês); usb.ids ~3 meses; linux-firmware tag 20260916 (gap 3 tags); regulatory.db 2026.09.03 (gap 1 release); SDIO 2.0.4 com torrent-first (mudança de canal); Intel microcode 20260812. **IDEA:** `tools/refresh_datasets.py` mensal (pci.ids+usb.ids+regulatory+WHENCE) + pins firmware por tag.

## Pós-tarefas
Todas as 6 ondas completas: `cargo check --release` global 0 erros; commits s393–s400; tags v1.9.99-s395→s400; `docs/memory/SESSION_INDEX.md` + `docs/memory/STATE.md` + `AGENTS.md` atualizados ao longo do programa; canvas em `docs/architecture/canvas-onda1-agentes-seguranca.md`, `canvas-onda2-kernel-core.md`, `canvas-ondas34-gpu-storage-rede.md`. Verificação ART/SIMD/ponteiros immutable confirmada soft-float-safe. Ajuste de placo: ondas de ideas aprenderam com lições s342–s361 (slog, HITL, boundary-codigo).

## Próximos (s401+): §10.1 fix download.py; pins firmware tag; refresh_datasets.py; bobina hostel.

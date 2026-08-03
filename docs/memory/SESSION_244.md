# SESSION_244 — NeuralFS fonte única (consolidação triplicata)

## Problema
NeuralFS existia em triplicata (padrão documentado como lição em SESSION_237, mas
sem guarda):
- `crates/k_nano/src/neural_fs/` — 12 arquivos canônicos, agent **morto** (294 linhas, nunca instanciado)
- `crates/hermes/src/neural_fs/` — 12 arquivos, cópia completa, agent avançado (686 linhas: USB opt-in, exFAT write, usb_trust, ecosystem tree, GPT virgin)
- `crates/neural-kernel/src/neural_fs/` — mod.rs re-exportava 9 módulos + cópias locais `neural_fs_agent.rs` (653 linhas, **vivo** no boot) + `tests.rs`

"Uma correção aplicada a uma cópia não chega às outras, e não há nada que avise."

## Diagnóstico
- k_nano é a crate base (R0); hermes (R3) e bin dependem dela → canônico = k_nano.
- O agent VIVO é o do bin (registrado no boot: `main.rs:1714` → `fs/mod.rs:168`).
- k_nano não pode implementar o trait `FilesystemAgent` dos rings (trait é ring-local:
  existe em bin, hermes E k_nano) → trait impl vira métodos `pub` inerentes + adapter
  `impl FilesystemAgent for k_nano::...::NeuralFsAgent` em cada ring (orphan rule OK).
- Todas as APIs usadas pelo agent avançado já existem em k_nano (`slog_bin!`, `fat32`,
  `exfat`, `exfat_write`, `usb_trust`, `gpt`, `globals::USB_MSC`, `ATA_DRIVER`).

## Fix
1. `k_nano/src/neural_fs/neural_fs_agent.rs` sobrescrito com o agent vivo do bin
   (comportamento de boot preservado), adaptado: `k_nano::slog_bin!`→`crate::slog_bin!`,
   `crate::USB_MSC`→`crate::globals::USB_MSC`, `impl FilesystemAgent`→5 `pub fn` inerentes.
2. `hermes/src/neural_fs/mod.rs` → facade `pub use k_nano::neural_fs::{...}`; 12 arquivos deletados.
3. `neural-kernel/src/neural_fs/mod.rs` → facade idêntica; 3 arquivos deletados.
4. Adapter `impl crate::fs::FilesystemAgent for k_nano::neural_fs::neural_fs_agent::NeuralFsAgent`
   adicionado no fim de `hermes/src/fs/mod.rs` e `neural-kernel/src/fs/mod.rs`.
5. **Guarda `tools/check_duplication.py`** (novo): exit 1 se o mesmo `.rs` (não-facade)
   existe em ≥2 crates. Ignora `mod.rs`/`lib.rs`/`main.rs` e facades puras de `pub use`.

## Verificação
- `cargo clean -p neural-kernel && cargo check --release` → **0 erros** (59s; warnings pré-existentes em ruvix/syscall).
- `python tools/check_duplication.py` → NeuralFS **ausente** da lista. Restam (dívida
  pré-existente, follow-up): camada `fs/*` (ata_agent/inference_fs_agent/mhi_scheduler
  em 3 crates; dev_fs/hermes_fs/log_fs/proc_fs/ram_fs hermes↔bin), camada net
  (netstack/network_agent/net/netdiag/netfs hermes↔bin), espelhos cortex/k_ai
  (cortex.rs, agents.rs, memory_agent, boot_log_agent, shutdown, multi_user), cópias
  idênticas (ntfs_reader, smp/spsc, tracer, verify), divergentes (interrupts, vga_buffer,
  audio/hda k_hal↔k_nano).

## Lição
A lição SESSION_237 valia, mas sem guarda o padrão se repetiu em ~50 arquivos. O guarda
agora faz "algo avisar". Consolidar o resto = follow-up (camada fs → k_nano; camada net;
espelhos cortex/k_ai), mesma técnica: fonte única na crate base + facade + adapter.

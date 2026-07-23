# SESSION_169 — Fix reboot HW: soft-reboot BOOT.LOG

**Data:** 2026-07-23  
**Foco:** Loop de reinício em HW real após saudação JARVIS.

## Causa raiz

`fat-boot-log` + `flush_bootlog_after_greeting` → soft-reboot (`0x64`/`0xCF9`) com magic `NEURLOG!`.  
Nenhum UEFI/bootloader escrevia `NEURDONE` → `init_from_phys` não armava `SKIP_FLUSH` → JARVIS → reboot de novo = **loop**.

Não confundir com crash/#PF: countdown `>>> BOOT.LOG via UEFI | reboot Ns` na tela confirma soft-reboot.

## Fix (produto)

1. `flush_bootlog_after_greeting` retorna `bool` — MSC/ATA flush ou ramlog+FB; **nunca** `0xCF9`.
2. `init_from_phys`: `NEURLOG!` → `SKIP_FLUSH` + marca `NEURDONE` local.
3. Feature `fat-boot-log-soft-reboot` / `k-nano/soft-reboot-bootlog` — default **OFF**.
4. `init_after_usb`: mensagem FB clara se MSC/ATA ausente.

## Verificação

- `cargo clean -p neural-kernel && cargo nk --features fat-boot-log` → 0 erros
- HW: passa K50 e **não** reinicia; Runtime/AgentFleet vivo
- Se reboot **antes** de K44: trilha secondary (#PF/SMP)

## Arquivos

- `crates/neural-kernel/src/boot_logger.rs`
- `crates/neural-kernel/src/audio/jarvis.rs`
- `crates/k_nano/src/boot_ramlog.rs`
- `crates/neural-kernel/Cargo.toml`, `crates/k_nano/Cargo.toml`

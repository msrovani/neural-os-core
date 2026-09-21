# Relatório (rev. 3) — Persistência de BOOT.LOG / NSGDB no pendrive de boot

**Data:** 2026-09-20 (rev. 3 — pós-bughunt)  
**Status implementação:** Trilha A **implementada** + **wire `init_from_phys`** (bughunt C1).  
`crates/logwriter-efi`, seal no reboot/panic, ESP `BOOTX64=logwriter` + `EFI/neural/limine.efi`, PMM reserve ramlog, consume `NEURDONE` no boot seguinte. Soft-reboot feature **removida**. Aceite metal: boot → reboot UI → `E:\BOOT.LOG` com linhas `[T+]`.

---

## 0. Resumo para decisão

| | |
|---|---|
| **Por que o stick ficava vazio** | Kernel só gravava via USB-MSC; no metal MSC não sobe → `live_usb_no_msc` bloqueia outros discos (correto). |
| **Trilha A (agora)** | Ramlog `@0x1000_0000` → seal `NEURLOG!` → reboot ordenado → `logwriter-efi` grava `BOOT.LOG` via SFS do firmware → `NEURDONE` → **`init_from_phys` consome no próximo boot**. |
| **Trilha B (aberta)** | Bring-up MSC/hub no metal. |
| **Honesty (rev. 3)** | `LOG_TO_FAT32` removido; PMM reserve ✅; `SESSION_BODY` removido; `init_from_phys` wired ✅; soft-reboot API removida. |

---

## Changelog rev. 3 (bughunt 2026-09-20)

| Item | Antes | Agora |
|------|-------|-------|
| `init_from_phys` | definido, **0 callers** | chamado pós-`PHYS_MEM_OFFSET` |
| PMM ramlog | report dizia ausente | **já reservado** em `main.rs` |
| UEFI writer | report dizia ausente | `crates/logwriter-efi` |
| soft-reboot stub spin | `request_flush_and_reboot` → ! | **apagado** |
| SESSION_BODY | never written | **apagado** |
| PHASE banners | antes do work + slog sempre ok | após work; sev=status |
| `mk_esp` fail | warning + uefi.img stale | **panic** + prune ESP tree |
| Limine vendor | 12.5.2 (extract) | **bump 12.9.0** (manual se download pendente) |
| uefi-rs | 0.34 | **0.35** (passo; 0.40 residual) |

---

## 1. Anatomia (atual)

### 1.1 Cadeia de gravação (DEV, `fat-boot-log`)

```
log()/log_quiet()  ──► PRE_FAT_BUF (≤512) + boot_ramlog::append
                                            │
                        FLUSH_EVERY / flush / ensure_persisted
                                            ▼
                                   persist_now (USB→virtio→ATA→AHCI→NVMe)
                                            ▼
                             overwrite_boot_log data-only

Cross-boot (metal sem MSC):
  seal_for_next_boot → reboot → logwriter-efi → BOOT.LOG → NEURDONE
  próximo boot: init_from_phys consome NEURDONE e zera buffer
```

Fontes: `crates/k_nano/src/boot_logger.rs` (`persist_now` 553–733, `overwrite_boot_log` 292–458, `ensure_persisted` 942–989), `crates/neural-kernel/src/main.rs` 1843–1890 (early), 2160–2283 (DriverInit).

**Conclusão factual:** `overwrite_boot_log` só retorna `Ok` depois de um `write_sectors == true`. Placeholder intacto ⇒ nenhum backend chegou lá. Não há “sucesso mentiroso” neste path.

### 1.2 Por que o único backend possível (USB_MSC) não existe no metal

Três tentativas, todas passam por `k_hal::usb::probe_and_install → UsbMassStorage::probe → probe_all_hcs → bringup_boot_msc` (`hub_msc.rs`):

| Momento | Budget | Bloqueio |
|---|---|---|
| Early (`main.rs:1870`) | 12 s metal | hub interno: `classify_root_port` → `Hub` → `try_msc_behind_hub` (GET_DESCRIPTOR hub, power, reset filho, `push_route`, TT). Qualquer falha ⇒ `mark_msc_port_failed(port)`. |
| DriverInit (`main.rs:2181`) | 12 s | mesmas portas; as marcadas `failed` são **puladas** (`msc_port_skipped`) — salvo `clear_msc_port_skips` no pass B. |
| Runtime (`SysInfoAgent` → `ensure_persisted`) | 8 s | (a) 1ª chamada com UI viva **só faz flush**, sem probe; (b) probes seguintes a cada backoff 50→3200 ticks; (c) `probe()` recusa multi-HC com UI viva → só HC bound; (d) cada probe é **síncrono dentro do tick** → até 8 s de UI congelada por tentativa. |

Somado a `heal_on_first_failure` (retorna cedo se `internal_disk_skipped && no_msc`) e `try_flush_ramlog` (idem), o sistema converge para: **UI viva, ramlog cheio de diagnóstico USB, nada no stick.**

Nota: sem foto do Hub/FB deste boot **não dá** para dizer *qual* passo do hub falhou. Esse é exatamente o dado que a Trilha A entrega.

### 1.3 O que já foi construído para o caminho de 2 estágios

| Peça | Estado | Onde |
|---|---|---|
| Ramlog físico 256 KiB, header `{magic,len,crc24+ckpt}` | ✅ | `k_nano/src/boot_ramlog.rs` |
| `NEURLOG!` (pendente) / `NEURDONE` (consumido) + consumo no boot seguinte | ✅ | `init_from_phys` |
| Selar CRC + warm-reset (0x64/FE → 0xCF9) | ✅ atrás de `soft-reboot-bootlog` (OFF) | `request_flush_and_reboot` |
| Reboot ordenado por UI/CAD | ✅ | `neural-kernel/src/shutdown.rs::begin_orderly_reboot` |
| Placeholder `BOOT.LOG` pré-alocado com texto “UEFI nao achou SFS” | ✅ | `tools/mkfat32.py:566-573` |
| **Escritor UEFI que copia ramlog → BOOT.LOG** | ❌ **nunca existiu** | — |
| Reserva de `0x1000_0000` no PMM | ❌ | só kernel/heap/stack em `main.rs:1448-1490` |

SESSION_169 desligou o soft-reboot porque “nenhum UEFI writer gravava NEURDONE → loop”. A decisão foi correta como *mitigação*; como *solução* faltou a peça.

---

## 2. Solução — Trilha A: `neural-logwriter.efi` (usa os drivers do firmware)

### 2.1 Ideia

O firmware que carregou `BOOTX64.EFI` do stick **já tem** xHCI + hub + MSC + FAT funcionando para aquele stick. Antes do `ExitBootServices` (que o Limine faz) esses drivers estão disponíveis via **Boot Services**. Logo: um pequeno app UEFI, executado **antes** do Limine, pode ler o ramlog deixado em RAM pelo boot anterior e escrevê-lo no volume `NEURAL-OS` com `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL`.

```mermaid
sequenceDiagram
    participant FW as UEFI firmware (xHCI+MSC+FAT do stick)
    participant LW as \EFI\BOOT\BOOTX64.EFI = neural-logwriter
    participant LM as \EFI\neural\limine.efi
    participant K as kernel (Limine protocol)
    participant RAM as RAM 0x1000_0000 (ramlog)

    FW->>LW: StartImage
    LW->>RAM: lê header; magic==NEURLOG! && crc ok?
    alt ramlog válido
        LW->>FW: LocateHandleBuffer(SimpleFileSystem)
        LW->>FW: abre volume que contém BOOT.LOG (ou label NEURAL-OS)
        LW->>FW: Write BOOT.LOG (+ BOOTLOG\boot_NNN.log histórico)
        LW->>RAM: magic = NEURDONE
        LW-->>LW: 1 linha no console: "BOOT.LOG gravado (N bytes)"
    else vazio / CRC fail
        LW-->>LW: "ramlog ausente/corrompido" (segue)
    end
    LW->>LM: LoadImage+StartImage (chainload)
    LM->>K: ExitBootServices → kernel_main
    K->>RAM: append() durante toda a sessão
    K->>K: reboot ordenado (UI/CAD/panic) → sela CRC + NEURLOG! → 0x64/FE
    Note over FW,RAM: próximo boot: logwriter grava o log desta sessão
```

### 2.2 Por que é a solução certa agora

1. **Independe do nosso xHCI.** É o mesmo stack que acabou de ler 20 MB de `kernel.elf` pelo hub interno.
2. **Reaproveita 90 % do que existe** (`boot_ramlog`, magic/CRC, consumo no `init_from_phys`, reboot ordenado). Falta o app UEFI + 3 ajustes no kernel.
3. **FAT feito pelo driver do firmware**: sem walk de root dir à mão, sem “data-only” — pode até **criar** `BOOTLOG\boot_0007.log` histórico e crescer o arquivo além de 256 KiB.
4. **Desbloqueia a Trilha B**: os `fb_usb(...)` do `hub_msc.rs` já vão ao ramlog; passam a chegar à IDE (`tools/mesh_log_parser.py`/`parse_boot_score.py`).
5. **AIOS-compatível** (ADR-0088 §4): não é bypass — é o segundo estágio previsto em SESSION_169, agora com o escritor que faltava.

### 2.3 Limites (honestos)

| Limite | Mitigação |
|---|---|
| Só persiste **no boot seguinte**; a sessão precisa terminar em **warm-reset** (não power-off). | Reboot ordenado pela UI/CAD já existe; adicionar `/reboot` e reboot no `panic`. Instruir: “no metal, encerre com reboot, não com o botão”. |
| RAM pode ser zerada/realocada pelo firmware no reset. | Endereço 256 MiB foi escolhido empiricamente (Note 1050); CRC24 + magic detectam; **espelhar** o header em 2º endereço (ex. `0x2000_0000`) custa 1 `memcpy` no selo. |
| Firmware pode não expor SFS do volume de dados. | EDK2 monta qualquer FAT12/16/32 em partição GPT/MBR. Fallback: escrever na **ESP** (`\NEURAL\BOOT.LOG`) — a ESP é FAT32 real e o Windows monta com letra. |
| Secure Boot | Já é pré-requisito OFF (Limine sem shim). Nada muda. |
| Sem log **durante** a sessão (só pós-mortem). | Aceitável para análise na IDE; a Trilha B continua sendo o caminho “ao vivo”. |

### 2.4 Escopo de implementação (estimativa: 1 sessão)

**Nova crate `crates/logwriter-efi`** (target `x86_64-unknown-uefi`, `uefi = "0.35"` com `alloc`, `panic_handler`; toolchain atual lista o target, falta `rustup target add x86_64-unknown-uefi`):

```rust
// esboço — não é código final
#[entry] fn main() -> Status {
    uefi::helpers::init().ok();
    if let Some(log) = ramlog::take_pending(0x1000_0000) {        // magic+crc24 (mesmo algoritmo do kernel)
        match fat::write_bootlog(&log) { Ok(n) => print("BOOT.LOG %d bytes"), Err(e) => print(e) }
        ramlog::mark_done();                                       // NEURDONE
    }
    chainload(cstr16!("\\EFI\\neural\\limine.efi"))                // LoadImage + StartImage (mesmo device handle)
}
```

- `ramlog::take_pending`: lê phys direto (Boot Services rodam identity-mapped; `0x1000_0000` está abaixo de 4 GiB) — **validar `EFI_MEMORY_DESCRIPTOR`** (`GetMemoryMap`) para não ler MMIO.
- `fat::write_bootlog`: `LocateHandleBuffer(ByProtocol, SimpleFileSystem)` → para cada volume: `open_volume` → `open("BOOT.LOG", READ|WRITE)`; 1ª que abrir ganha; grava `[S] …` + payload; `flush`. Opcional: `BOOTLOG\boot_<ckpt>_<n>.log` (create).
- Sem SFS do volume de dados → tenta ESP (`\NEURAL\BOOT.LOG`, create).
- Chainload: `LoadImage(false, self_handle, device_path(ESP, path), None, 0)` + `StartImage`. Se falhar, mensagem e `Stall` (não brickar: firmware volta ao menu de boot).

**Build/imagem**

- `crates/boot/build.rs`: compila `logwriter-efi` (bindeps, `target = "x86_64-unknown-uefi"`, mesmo padrão do kernel); no ESP tree: `EFI/BOOT/BOOTX64.EFI` = logwriter, `EFI/neural/limine.efi` = Limine vendor. `mk_esp_fat.py` já suporta LFN/subdirs (bucket `EFI/BOOT`; adicionar `EFI/neural`).
- `limine.conf`: inalterado (`boot():/kernel.elf` resolve pelo device handle do Limine, que é o mesmo ESP).
- QEMU/OVMF: mesmo fluxo — vira **teste de regressão** (`-d int` para o chainload; ramlog visível após `system_reset` no monitor).

**Kernel (3 ajustes pequenos, todos em k_nano/bin — sem lógica nova no bin)**

1. `frame_allocator.reserve_range(BOOT_RAMLOG_PHYS, BOOT_RAMLOG_CAP)` em `main.rs` junto das outras reservas (fecha o achado SESSION_330 “clobberable”).
2. `boot_ramlog::seal_for_next_boot()` = o miolo de `request_flush_and_reboot` **sem** o reboot (CRC + `NEURLOG!` + `sfence`), chamado em `begin_orderly_reboot`/`begin_orderly_shutdown` e no `panic` handler antes do reset. Não reintroduz o loop: o kernel já consome `NEURLOG!` em `init_from_phys` e o logwriter grava `NEURDONE`.
3. `init_from_phys`: quando encontra `NEURDONE`, logar `BOOT.LOG do boot anterior gravado pelo logwriter (K<ckpt>)` com sev `ok` (evidência cross-boot no próprio log).

**Aceite**

- QEMU: boot → `/reboot` → 2º boot mostra `BOOT.LOG gravado` no console UEFI e `disk_qemu.raw:/BOOT.LOG` contém `[T+…]` do 1º boot.
- Metal: boot → reboot pela UI → Windows: `E:\BOOT.LOG` deixa de ser placeholder; `nonzero > 167`; linhas `USB: …` presentes → alimentam a Trilha B.

---

## 3. Trilha B — corrigir o MSC no kernel (o caminho “ao vivo”)

Só faz sentido **depois** de ter o ramlog na IDE. Com ele, a primeira coisa a olhar:

| Linha esperada no ramlog | Interpretação | Próxima ação |
|---|---|---|
| `USB: nenhuma porta CCS` | PP=0 pós-HCRST ou stick USB3 sem retrain (SESSION_316/327) | RMW PP=1 todas as portas + settle 500 ms já existe (pass B) — conferir se roda antes do budget |
| `USB: try root P<n>` … `hub class @ root` … `hub slot=… ports=…` … `hub port k … reset TIMEOUT` | Hub OK, reset da porta filha não completa | Ler `wPortStatus` C_PORT_RESET vs `PORT_ENABLE`; hubs USB3 usam bit 5 (BH reset) — `try_msc_behind_hub` só trata bits 1/4 |
| `hub child addressed … sem interface MSC BOT` | Stick é UAS (class 8/subclass 6/proto 0x62), não BOT | `parse_msc_config` aceitar só BOT `0x50`; UAS exige stream pipes — fallback: alt-setting BOT se existir |
| `MSC OK … SCSI falhou` | BOT sobe, SCSI READ CAPACITY/INQUIRY falha | STALL/clear feature; TUR loop; CSW tag |
| `USB: MSC budget abort (hub pass)` | 12 s não bastam com hub + USB3 | budget por **fase**, não total; ou mover hub pass para Runtime (agente `UsbHostAgent` EventDriven) |

Regras já aprendidas que se aplicam: RTS Interrupter +0x20, IOC DW3 bit 5, CC `Success=1/Short=13`, CSZ, scratchpads, PP=1 RMW (SESSION_313/316/327).

Além disso, dois ajustes independentes do diagnóstico:

- **`ensure_persisted` 1ª chamada com UI viva só faz `flush()`** e, sem backend, incrementa `FAIL_STREAK` (backoff cresce sem nenhum probe ter acontecido). Trocar por: armar `LAST_MSC_PROBE_TICK = now` **sem** chamar `flush()` quando `!storage_available()`.
- **Probe síncrono dentro do tick com budget 8 s** congela a UI. Se a Trilha B avançar, o probe deferido deve ser um agente com fatias (`poll_slice`), não uma chamada de 8 s.

---

## 4. Correções de honesty (independentes das trilhas)

| Item | Problema | Fix |
|---|---|---|
| `CONFIG.TXT` `LOG_TO_FAT32=1` | 0 referências em `.rs`; operador acredita que “liga” o log | Remover de `mkfat32.py`/`populate_fat32.py`/`HOWTO.md` **ou** implementar leitura via MSC/ESP. Recomendo remover (compile-time `fat-boot-log` é a verdade). |
| Texto do placeholder | “UEFI nao achou SFS” descreve um estágio que nunca existiu | Atualizar para “kernel não gravou (sem USB-MSC) — ver logwriter” até a Trilha A entrar; depois volta a ser verdadeiro. |
| `0x1000_0000` não reservado | PMM pode entregar o frame ao heap/DMA (modelos grandes, e1000 RX) → ramlog corrompido silenciosamente | `reserve_range` (Trilha A item 1) — fazer **já**, mesmo sem o logwriter. |
| `SESSION_BODY` | static declarado, nunca escrito; `PRE_FAT_BUF` cap 512 linhas | Ou remover `SESSION_BODY`, ou passar a acumular nele o excedente (até `BOOT_LOG_CAP`). Hoje o BOOT.LOG DEV = 512 primeiras linhas. |
| `note_persist_ok` só na 1ª gravação (K192) | ok | — |

---

## 5. Ordem recomendada

1. **Hoje (sem imagem nova):** anotar/fotografar Hub `usb`/`bootlog` no próximo boot metal — confirma H1 em 30 s.
2. **Sessão seguinte — Trilha A completa** (crate `logwriter-efi` + build.rs + 3 ajustes kernel + reserva PMM) + honesty §4. Aceite QEMU → imagem HW → metal → `E:\BOOT.LOG` real.
3. **Com o ramlog na IDE — Trilha B**: classificar a falha pelo §3 e atacar o passo exato; medir com o mesmo pipeline (boot → reboot → ler stick).
4. **Depois:** `UsbHostAgent` EventDriven para o probe deferido (tira os 8 s de dentro do tick); FileFlash remount quando MSC subir (já existe `remount_after_usb_msc`, só precisa do MSC).

---

## 6. Referências

- Código: `k_nano/src/boot_logger.rs`, `boot_ramlog.rs`, `usb_msc.rs`, `xhci/bringup.rs`; `k_hal/src/usb/{mod,hub_msc}.rs`; `neural-kernel/src/{main,boot_logger,shutdown}.rs`; `hermes/src/agents/sysinfo_agent.rs`; `crates/boot/build.rs`; `tools/limine/mk_esp_fat.py`; `tools/mkfat32.py`.
- Sessões: 169 (soft-reboot loop / escritor UEFI ausente), 170 (BOT/SCSI), 264 (feature + data-only), 269 (breaker), 295/296 (live USB), 312/313/314/316/327 (xHCI metal), 330 (ramlog clobberable), 345 (mesmo sintoma Alienware).
- Evidência: `logs/metal-e/` (2026-09-20).

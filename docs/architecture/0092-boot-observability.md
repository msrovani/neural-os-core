# ADR-0092: Observabilidade de boot — dmesg Neural, canais e placar

**Data:** 2026-08-24  
**Status:** Accepted  
**Lifecycle (INDEX):** `fazendo`  
**Depende de:** ADR-0002 (VGA/serial — canais físicos), ADR-0039 (8 fases), ADR-0041 (`slog` anéis), ADR-0042 (boot N0–N5 + honesty LOADED/ABSENT/FAILED), ADR-0088 (Observe→Plan→Act→Verify, sem bypass)  
**Não substitui:** ADR-0039 (ordem das fases), ADR-0042 (o que o boot *faz*). Este ADR define **como o boot se relata**.  
**Lacuna 0090–0099:** INDEX reservava a faixa para ADR-0100. **0092 é exceção temática** (contrato de log), pedida pelo maintainer; **não** preencher 0090/0091/0093–0099 sem decisão nova.  
**IDEA:** #539  
**Sprint / TODO:** ondas **O0–O5** neste ADR (não misturar com T-001–T-075 do 0100).

---

## 1. Contexto

O Runtime e o compositor Jarbas já sobem. O **relato** do boot não é um dmesg: é um dump de instrumentação de sessão.

Sintomas (QEMU WHPX, logs `logs/boot_k49_*.txt`, UI 1280×800):

1. **Vários canais sem contrato** — `slog_*!`, `println!`, `boot_ckpt(Knn)`, ramlog/`BOOT.LOG`, EventBus `BOOT_PHASE`, `INIT1: rN poll`, HUD compositor, TTS/chat.
2. **Serial ≠ tela** — serial parece produto; FB mostra `K49:` / debug até `claim_graphics()`, depois HUD com cara de laboratório (`NET no-llm MoE`).
3. **Duplicação e ordem falsa** — mesmo facto em slog + `boot_logger` + println; `T+0` até o PIT; `Fat32Reader::new` loga BPB em todo lookup; loops de modelo (`llama8b` ×10); SMP INIT/SIPI TRACE; e1000 MMIO a cada prova.
4. **Ckpts reciclados** — `boot_ckpt` usa `u8` (K22, K43, K49 várias vezes). Não é sequência monotónica.
5. **Fase 8 clandestina** — EventBus cobre 0–7; models/BPE/STT/greeting correm depois de Runtime sem banner.
6. **IA não consegue triar** — `ok`, `warn`, `fail` e TRACE de registrador usam o mesmo slot `[sub]` (`info`, `trace`, `ckpt`). `BootReport::finalize_and_publish` existe (`k_nano::boot_report`) mas não emite um placar parseável.

Referência de outros sistemas (não copiar código; copiar *disciplina*):

| Sistema | Padrão |
|---------|--------|
| Linux | um `printk` + *level*; console ≠ ring buffer; 1 linha por subsistema no fim |
| Redox | `kstart` → logger → `kmain`; `log` crate; serial ≠ login |
| Theseus | `nano_core` mínimo → **`captain`** (ordem única); ecrã só após WM |
| U-Boot bootstage | fases nomeadas + duração |

**Premissa AIOS (0088):** o boot continua a decidir com Observe→Verify. O log **é** essa evidência. Dump TRACE no path quente não é “IA desde o boot”; é ruído.

---

## 2. Decisão

### 2.1 Três canais (nunca misturar papéis)

| Canal | Destino | Conteúdo |
|-------|---------|----------|
| **A — dmesg** | COM1 + `BOOT.LOG` + ramlog | Uma linha por evento, parseável. Default consola = `WARN`+`FAIL`+`OK` de fase. `TRACE` só com `log_level=trace` / feature `boot-trace` / QEMU explícito. |
| **B — produto** | Framebuffer GOP | Antes do compositor: **no máximo uma linha por fase**. Depois: HUD de produto (marca, net, voz, confiança). Sem `Knn:`, sem `INIT1`. |
| **C — placar** | Serial + card/HUD 6–12 linhas | Bloco `=== BOOT SCORE ===` no fim do bring-up e opcionalmente a cada N ticks. Fonte única: `BootReport` alargado. |

`serial_println` / `_print` (`k_nano::serial`): COM1 presente → **não** pintar FB com dmesg. FB de boot = banners de fase + placar (via API explícita, não eco de slog).

### 2.2 Vocabulário de severidade (substitui o `[sub]` livre)

O 4º campo do `slog!` deixa de ser texto livre (`info`/`ckpt`/`ata`). Valores canónicos:

| `sev` | Significado | Consola default | Ficheiro |
|-------|-------------|-----------------|----------|
| `ok` | Invariante cumprida neste profile | sim | sim |
| `warn` | Degradado ou esperado-mas-atenção | sim | sim |
| `fail` | Bug / invariante partida | sim | sim |
| `trace` | MMIO, SIPI, BPB, dumps | não | sim se `boot-trace` |

Formato de linha (A), estável para grep/IA:

```text
[T+N] [Rn] [crate] [src] [sev] - texto
```

`src` = item estável (`e1000`, `smp`, `fat32`, `BOOT`). Não usar `src` para a mensagem.

### 2.3 Capitão visível (fases)

Manter as 8 fases de ADR-0039. Cada `publish_boot_phase` imprime **uma** linha banner no canal A e, se `!GRAPHICS_OWNED`, uma linha no canal B:

```text
=== PHASE n=<0..7> name=<SafeHarbor|...|Runtime> status=<ok|warn|fail> ===
```

Acrescentar **fase 8 `PostRuntime`** (não EventBus 0–7): load de modelos, BPE, STT, greeting, `finalize_and_publish`. Hoje isto é clandestino no `main.rs` após Runtime.

Não criar um segundo capitão. A ordem continua em `kernel_boot` / `neural-kernel`. k-nano **não** vira Theseus `captain`; só deixa de logar como se cada driver fosse o boot inteiro.

### 2.4 Placar (contrato para humano e IA)

Emitido por `k_nano::boot_report::finalize_and_publish` (e re-emit no scheduler se campos mudarem). Campos mínimos:

```text
=== BOOT SCORE qemu=<bool> ram_mb=<n> smp_online=<n> ===
phase_0_7     <ok|fail>
cpu           <ok|warn|fail>  online=  pollable=
net           <ok|warn|fail>  nic=  rx=
storage       <ok|warn|fail>  bus=
llm           <ok|degraded|fail|await>
audio_stt_tts <ok|degraded|await>
gpu           <ok|await|fail>
wifi          <await|ok|fail>
attention     <lista curta ou "none">
===
```

Regras de leitura:

| Classe | Acção da IA / maintainer |
|--------|---------------------------|
| `ok` | Não investigar. |
| `degraded` | Só se **não** `expected` para o profile. |
| `fail` | Investigar (panic, #PF, net gate down, storage wipe). |
| `await` | Silício ausente — não é bug. |
| `attention` | Única lista de follow-up. |

**Profiles:** `qemu` vs `hw` (`platform_probe::hypervisor`). Exemplos: skip PIO GB no hypervisor = `degraded expected`; LLM ABSENT no QEMU sem loader = `degraded expected`; `ap_pollable=false` = `warn` conhecido (ADR-0057), não `fail`.

### 2.5 Ckpt `Knn`

`boot_ckpt` deixa de pintar o FB após esta ADR (já gated por `GRAPHICS_OWNED`; alargar: **nunca** pintar K* no produto). Serial: substituir por slog `sev=trace` ou por `phase.step` monotónico (`5.12`) se ainda precisar de freeze-debug. Reciclar o mesmo `u8` é **proibido** para semântica de progresso.

### 2.6 Anti-ruído (obrigatório, não polish)

| Fonte | Política |
|-------|----------|
| FAT BPB | Uma vez por mount, não por `lookup`. |
| `INIT1: rN poll` | Só se Oneshot **estourar timeout**; senão TRACE. |
| e1000 RDH/RDT / desc raw | `trace`. Uma linha `ok` no bind + prove RX. |
| SMP INIT-SIPI ICR | `trace`. Resumo: `Brought up N APs`. |
| Scan de nomes de modelo | Uma linha de skip por família, não ×10. |
| PnP Hermes | Não TTS / não chat. EventBus + slog. |
| Path quente (tick NIC) | Sem slog `ok` repetido; no máximo 1/N ticks em `trace`. |

### 2.7 Non-goals

- Reescrever todos os `slog_*!` numa PR (migração por onda; default: `sub` desconhecido trata-se como `trace` até classificado).
- Segundo logger, JSON no path quente, ou syslog RFC.
- Mudar a ordem de bind de drivers (isso é 0039/0088/H1).
- Declarar v2.0.0.

---

## 3. Plano de correção (ondas O0–O5)

Implementação nas crates do anel certo (`k_nano` slog/report; bin só `publish_boot_phase` / wire; `jarbas` HUD). Sem cópia no bin.

### O0 — Contrato no slog (S, R0)

- Estender `slog!` / `_print`: filtro `CONSOLE_SEV` (atomic); `trace` não ecoa na consola default.
- Documentar `sev` ∈ {ok,warn,fail,trace}; helpers `slog_ok!` opcionais depois.
- Teste host: linha com `fail` passa no filtro; `trace` não (simular buffer).

**Aceite:** `cargo test -p k-nano` (filtro) + `cargo check --release` 0 erros.

### O1 — Capitão + fase 8 (S, bin + k_nano)

- Banner `=== PHASE n= name= status= ===` em `publish_boot_phase`.
- Uma linha FB por fase se `!GRAPHICS_OWNED`.
- Marcar PostRuntime (models/BPE/greeting/finalize) com banner `n=8`.

**Aceite:** log QEMU contém exactamente 9 banners (0–8) uma vez cada (DriverInit pode ter sub-status no **mesmo** n, não 12 `publish` como fase nova — colapsar payloads extras para `trace` ou uma linha `phase=5 step=`).

### O2 — Mudos (S–M, k_nano + bin + jarbas)

Aplicar tabela §2.6 nos hotspots: `fat32.rs`, `e1000.rs`, `smp/mod.rs`, `xhci/bringup.rs`, `main.rs` INIT1, scans FAT de modelo, compositor HUD.

**Aceite:** boot QEMU NoDisk ou mini: serial **< 400 linhas** até Runtime (hoje milhares); zero BPB repetido; zero INIT1 em boot saudável.

### O3 — Placar (S, k_nano `boot_report`)

- Alargar `BootReport` (cpu/net/storage/llm/profile).
- `finalize_and_publish` imprime o bloco `BOOT SCORE`.
- Classificar `expected` via hypervisor.

**Aceite:** o bloco aparece uma vez no serial; um parser host (`tools/parse_boot_score.py`) extrai `attention` e exit 0 se só `degraded expected`.

### O4 — FB produto (S, jarbas)

- HUD: retirar jargão de laboratório do título permanente; mover MoE/no-llm para TRACE ou placar.
- Confirmar `boot_ckpt` não pinta após claim **nem** antes (só banner de fase).

**Aceite:** screenshot/screendump: sem `K49:`; HUD legível por humano.

### O5 — Profile qemu vs hw (S, evidência)

- Documentar no placar `qemu=true|false`.
- Mesmo código; políticas de skip (PIO GB, xHCI timeout) já existentes **devem** aparecer como `degraded expected`, não `fail`.

**Aceite:** um log QEMU e um log metal (quando houver) parseados pelo mesmo script; `attention` diferente e honesto.

---

## 4. Ficheiros (âncoras)

| Peça | Path |
|------|------|
| slog | `crates/k_nano/src/slog.rs` |
| eco serial | `crates/k_nano/src/serial.rs` (`_print`) |
| report | `crates/k_nano/src/boot_report.rs` |
| fases | `crates/neural-kernel/src/main.rs` (`publish_boot_phase`) |
| ckpt | `crates/jarbas/src/display/fb.rs` (`boot_ckpt`) |
| HUD | `crates/jarbas/src/display/compositor.rs` |
| INIT1 | `crates/neural-kernel/src/main.rs` (~registry init_trace) |
| parser | `tools/parse_boot_score.py` (O3) |

---

## 5. Consequências

**Positivas:** serial útil para IA e maintainer; ecrã de produto; freeze-debug continua via `boot-trace`; `BootReport` deixa de ser morto.

**Negativas / risco:** quem debuga com `K49:` no FB perde o hábito — usar serial TRACE + ramlog. Primeira onda pode esconder TRACE necessário: default conservador (`warn`+ visível; `ok` de fase visível).

**Relação ADR-0002:** dual VGA+serial permanece a *infra*. Este ADR redefine *política*: GOP/compositor = canal B; COM1 = canal A. VGA text 80×25 é fallback, não o desktop.

**Relação ADR-0100:** observabilidade não entra nas ondas T-xxx. Executar O0–O3 **antes** de mais instrumentação SMP/NIC no tick.

---

## 6. Critério de fecho da ADR

Lifecycle → `completa` quando O0–O4 ✅ em QEMU (evidência `docs/evidence/` ou `logs/` + SESSION) e O5 documentado (metal pode ficar `await` no placar). Sem isso, lifecycle permanece `fazendo` após O0 iniciado.

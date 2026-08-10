# SESSION_258 — Auditoria 10 itens: Fases 1–4 (memória, Onda CPU, OTA assinado) (2026-08-10)

**Escopo:** Execução do plano de correções da auditoria (10 itens), organizado por
complexidade crescente, com gate QEMU entre fases. Fases 1–4 entregues; a Fase 5
(auto-observação unificada, #9) foi avaliada — o primeiro passo dela (loop Onda
CPU → SELF.STATE) entrou na Fase 3; o restante é marco futuro (sessão dedicada).
**Status:** Fases 1–4 concluídas e validadas — `cargo check --release` 0 erros,
188 testes host (3 novos de assinatura), boot QEMU TCG (config do CI) alcança
`[BOOT:Runtime]` com 0 `[EXC]` e SMP ativo.
**Branch:** `fix/auditoria-10-itens` (não mergeada).

---

## 1. Fases executadas

| Fase | Item | Entregue | Evidência |
|---|---|---|---|
| **F1** | #3 bridge tautológico | `topics_in_sync`/`settings_contract_ok`/`log_bridge_status` removidos; contrato documental via re-export (`crate::audio` = `jarbas_crate::audio`) | jarbas_bridge.rs + main.rs |
| **F1** | #8 logs frame[] | Log do SESSION_252 gateado atrás da feature `heap-trace` (OFF default) | allocator.rs + Cargo.toml |
| **F1** | #6 evidência boot | `tools/compress_boot_evidence.py` (digest PASS/FAIL); log de 1036 linhas → digest + gzip em `docs/evidence/archive/` | script + evidência |
| **F1** | #5 smoke CI endurecido | Greps negativos: `[SELF-HEAL]`, `CR2=0xfffffffffffffff8` (triple), 4+ `[EXC]` (tolerância do kernel = 3) | ci.yml + boot.yml |
| **F2** | #10 stall do boot | **Causa raiz real** (não era o wrap 2⁶⁴): `heap_start` desalinhado porque o padrão `*(.bss .bss.*)` engolia a seção `.bss.heap`; fim da região crescida caía 0x698 bytes numa página desmapeada → #PF em `k_ai::sgdb::art::insert_rec`. Fix: seção `.heap` (NOLOAD — rustc emite custom non-`.bss*` como PROGBITS) + `ALIGN(MAXPAGESIZE)` + `grow_bump_auto` cobre a página do último byte (`(extra + base_off).div_ceil(4096)`) | limine.ld + allocator.rs; boot passa do grow (512→768→1024→1280 MB, 0 [EXC]) |
| **F2** | #1 map_page_direct | Contrato verificável: retorna `bool`; HUGE_PAGE validado por cobertura (`huge_page_covers`); PTE presente com outro phys = conflito → `false` (nunca alias); call sites contam só páginas mapeadas (rollback) | allocator.rs; boot OK |
| **F2** | #2 TSS/GDT | Opção (a): lazy_static TSS eliminado → `init_bsp_tss()` explícito (stacks estáticas), ordem garantida em `init_idt()`; GDT referencia `TSS_ARRAY[0]` com **hard-gate** (ISTs zerados → halt com mensagem, classe SESSION_250) | interrupts.rs; boot SMP (-smp 2) com AP `[R1]` ativo, 0 [EXC] |
| **F3** | #4 Onda CPU /hw/* | Loop fechado: `sgdb::hw_get(key)` (leitura) + consumidor no boot — releitura de `/hw/cpu/isa` alimenta `hw_profile` do SELF.STATE (ADR-0086 "boot é releitura"), fallback live se SGDB off | store.rs + mod.rs + main.rs; log `Onda CPU loop OK: /hw/cpu/isa=scalar (releitura)` |
| **F4** | #7 OTA sem autenticidade | Assinatura Ed25519 obrigatória: `tools/ota_sign.py` (RFC 8032 puro, zero deps, selftest contra vetor 1); `serve_update.py --sign-key` assina o digest sha256 no manifest (`sig`); kernel rejeita manifest sem `sig` válida da **release key pinada** (nova chave adicionada a `TRUSTED_PUBLIC_KEYS`; `verify_update_signature` aceita só trusted, nunca session pk). Seed local em `target/ota_release_seed.hex` (gitignored, NUNCA commitar) | identity.rs + self_update.rs + serve_update.py; 3 testes host (RFC vector, reject garbage, end-to-end com seed via `OTA_RELEASE_SEED`); PIN VERIFY True / TAMPER False |

## 2. Bugs reais pegos (não contornados)

1. **#10 (boot smoke vermelho há dias):** o CI travava pós-`auto-grow 512→768MB` com
   #PF em `rep movsq` (`CR2` = 1 página além do fim mapeado). Diagnóstico empírico
   (pfwalk/dump_pt_walk no `heap-trace`): a pte da última página do grow não estava
   presente no momento do fault, mesma CR3. Raiz: `heap_start` desalinhado (o linker
   engolia `.bss.heap` no `.bss` comum) → o tail da região crescida (0x698 bytes) era
   usável pelo allocator mas não mapeado. O wrap 2⁶⁴ (offset ~2044MB, caso do 2B)
   **continua aberto** — é um bug separado.
2. **#1:** `map_page_direct` early-returnava em HUGE_PAGE sem mapear nem reportar —
   `grow_bump_auto` "verificava" com `heap_pte_present` (que lê a entrada gigante como
   present) e avançava `HEAP_LIMIT` sem páginas reais.
3. **#7:** OTA integridade hash-only (self-consistent: MITM forja sha256 do próprio
   blob); agora exige assinatura da release key sobre o digest.

## 3. Decisões e limitações

- **Chave OTA release:** gerada na sessão (`tools/ota_sign.py --gen`); PUBKEY pinada
  em `identity.rs`; SEED em `target/ota_release_seed.hex` (fora do git). Rotação:
  novo par + nova entrada no pin + assinar com a nova seed.
- **Fase 5 (#9) adiada:** séries temporais runtime + export NMD1 para o neural-sgdb =
  1–2 semanas; o loop Onda CPU → SELF.STATE (Fase 3) é o primeiro passo.
- **Baseline do CI continua vermelha por razões pré-existentes:** (a) teste
  `hwexpert_v6_matches_v5_predictions` usa `include_bytes!` de modelos deletados em
  `372afd6` (restaurados localmente só p/ validar: `models/hw_expert/hw_expert_v4.bitnet`
  + `tools/target/hw_expert_v6.bitnet` gerado — untracked); (b) grep `"Phase 6"` do
  smoke obsoleto (o kernel atual loga `[BOOT:Runtime]`/`[BOOT:AgentFleet]`).

## 4. Instrumentos novos (heap-trace, OFF default)

- `dump_pf_walk` no handler REAL de #PF (`interrupts_ext.rs` — o patch_idt sobrepõe o
  do k_nano): caminha a page table da CR2 no momento do fault + dump de return
  addresses do stack (identificou `insert_rec`).
- `dump_pt_walk` no `grow_bump_auto`: estado das entradas e3/e2/e1/pte no grow.
- Features: `k-nano/heap-trace` e `neural-kernel/heap-trace` (wired).

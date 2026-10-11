# SESSION_465 — Follow-up auditoria USB: correlação EP0 + CDB + DMA reutilizável + log de estágio BOT

**Data:** 2026-10-11 · **Branch:** main (working tree) · **Escopo:** itens 2–4 da revisão follow-up (enumeração/transporte; HW itens 1/5 sem código)

## Contexto

Revisão follow-up após s464: (2) falso positivo em `wait_transfer_event()` (aceitava qualquer type-32 com CC ok, sem correlacionar TRB/slot/endpoint); (3) CDB anunciado como 16 B p/ comandos de 6/10 B + CSW sem tag/resíduo + sem log de estágio; (4) DMA por-transferência (4 MiB p/ verify completo de 256 KiB). CSW tag+resíduo já estava no tree (s464) — confirmado, sem retrabalho. Reset xHCI e teste USB 2.0 são procedimentos de HW, sem código.

## Verificação prévia

- `wait_transfer_event` idêntico ao `main` público no tree (bringup.rs intocado em s464) — achado procede.
- Só 3 call sites usam `wait_transfer_event` (fetch `tr_va` idêntico nos 3) — escopo fechado.
- `route_isoc_event` era privado em `mod.rs` — necessário p/ rotear alheios no EP0.

## Fixes

1. **Correlação EP0** (`xhci/bringup.rs`, `xhci/mod.rs`): `wait_transfer_event(slot, status_pa, timeout)` casa TRB pointer + slot + DCI 1 + CC; alheios roteados (isoc) ou ignorados. Falha loga slot/CC/residual; timeout loga. `drain_stale_ep0_events(slot)` pré-doorbell nos 3 sites — anel EP0 reutilizado em offsets fixos ⇒ mesmo PA entre transferências, só o dreno distingue obsoleto de atual. Status offsets: +16 (no-data, set-config), +32 (control-in). `tr_pa` adicionado ao `XhciState`; `route_isoc_event` virou `pub(crate)`.
2. **CDB explícito** (`usb_msc.rs`): `bot_transfer(..., cdb_len, ...)` — TUR/INQUIRY/SENSE=6, CAPACITY/READ10/WRITE10/SYNC=10 (7 callers); clampa em 16 e zero-padding.
3. **DMA reutilizável** (`usb_msc.rs`): `dma_pa` persistente por dispositivo (1 página, lazy alloc); `free_dma` removido (superado). Verify 256 KiB = 0 churn (antes ~4 MiB alloc/free).
4. **Log de estágio BOT**: `cbw/data-in/data-out/csw/csw-content` com slot+opcode (+CC+resíduo) nos 4 retornos de falha.

## Deferido com motivo

**Mass Storage Reset + Clear Halt**: exige plumbing de EP0 mps/interface + toggle reset — muda estado do dispositivo e não há como provar correto sem HW. Reset errado trava o device; `disable_slot` + retry limitado segue honesto e limitado. Passo pós-enumeração, com HW para validar.

## Validação host

- `cargo check --release` → 0 errors.
- `cargo test -p k-nano --lib -- --test-threads=1` → 386 passed, 0 failed (sem testes novos neste round — lógica é bare-metal/MMIO, não mockável em host sem virar teste de faz-de-conta).

## Lições

1. **Anel reutilizado em offset fixo invalida correlação só-por-PA** — EP0 reusa os mesmos TRBs; evento antigo casa por PA. Dreno pré-doorbell é o que torna a correlação válida, não o match em si.
2. **Comprimento anunciado ≠ tamanho do buffer** — CBW carrega bloco de 16 B mas `bCBWCBLength` diz o válido (6/10); anunciar 16 é comando malformado p/ unidades estritas.
3. **Reuso supera alloc/free em caminho quente** — 1 página persistente por dispositivo elimina churn e vazamento de uma vez; `free` por-transferência era o remendo, não a solução.
4. **Log de estágio antes de recovery** — "SCSI não respondeu" sem estágio (cbw/data/csw) não diagnostica; recovery (Reset) sem diagnóstico validado é roleta. Ordem: observar → correlacionar → só então recuperar.

## Pendentes (HW)

- Captura PRE/POST dos dois xHCI (00:14.0) + teste USB 2.0 conhecido (procedimentos, sem código).
- Encerramento s464 mantido: gravação conhecida + readback + flush + recovery pós-reset + origem registrada.

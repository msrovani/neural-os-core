# SESSION_464 — Regressão APIC EXTD (s463) corrigida + integridade de evidência QEMU

Data: 2026-10-10. Contexto: pós-ciclo QEMU 1–3 do s463, o relato do orquestrador afirmou
"fallback silencioso WHPX→TCG sob pressão de memória". Recons (2 explorers) refutaram o
relato e revelaram uma regressão real introduzida pelo próprio s463 (commit `61f8b1e4`).

## Achados

1. **Não houve fallback algum.** `run-qemu-p2p-mesh.ps1:34` tem default `-Accel tcg`;
   os args notes dos QEMU1/2/3 nunca passaram `-Accel`; o guest mostrou `hv=TCG` desde
   T+0 (sem fase MicrosoftHv). A degradação foi ergonomia: default TCG + nenhum launcher
   gravava a linha de comando usada (`*.args.txt` eram notas manuais sem `-Accel`).
2. **REGRESSÃO s463 — gate APIC com bit errado.** `apic.rs` testava IA32_APIC_BASE
   **bit 11** como "firmware já em x2APIC"; bit 11 = **EN** (xAPIC global enable — sempre 1
   em máquina saudável). O enable de x2APIC é **bit 10 = EXTD** (Intel SDM Vol 3A §10.4.4;
   o diag do repo também tinha `en=`/`extd=` trocados). O early-return fail-closed novo do
   s463 (`firmware_x2 && !hv_allows_x2` → "LAPIC intocado") disparava em **todo boot QEMU**
   (WHPX e TCG têm hv_allows_x2=false) → `USING_APIC=false` → `init_smp` → "APIC nao
   disponivel — SMP ignorado" → `smp_online=1`. Sweep de logs: **10-04/10-05 = smp_online
   4..6** (24+ boots WHPX, mesmo path); **10-10 = smp_online=1 em TODOS** (M5, mesh4,
   QEMU3). As validações s463 rodaram BSP-only sem notar. Janela: WIP s463 em tree já às
   02:50 (mesh_verify 02:50 e M5 09:14 já smp_online=1; commit 11:19).
3. **Launchers sem evidência.** `run-qemu-whpx.ps1`: retry TCG era **dead code** (PS 5.1
   não lança exceção em exit≠0 de exe nativo — SESSION_251); serial logs de runs
   consecutivos se **sobrescreviam** (QEMU1/QEMU2 perdidos para QEMU3); **zero** preflight
   de RAM em qualquer launcher (os "gates" dos lab scripts forçavam overcommit e seguiam).

## Fixes

- `crates/k_nano/src/apic.rs`: `apic_base_extd()` lê EXTD (bit 10); labels EN/EXTD
  des-trocados em `smp_trace_apic_mode`, readback e `lapic_timer_diag`; `was_x2` no bit 10;
  teste regressão `apic_base_extd_reads_bit10_not_en` (casos: 0xfee00900 (QEMU baseline)
  NÃO é x2 → UseMmio; EXTD=1+WHPX → FailClosedBspOnly preservado). Política fail-closed e
  hardening CAS do s463 intactos. k-nano apic 12/12; `cargo check --release` 0 erros.
- `tools/qemu_mem_preflight.ps1` (NOVO): `Get-QemuHostFreeGB` / `Test-QemuWhpxAvail`
  (cache) / `Assert-QemuMemFree -NeededGB [-Force]` — sem -Force = banner + exit 1;
  -Force = overcommit WARN.
- `run-qemu-p2p-mesh.ps1`: preflight (n×Mem+1G); **args dump por nó**
  (`logs/mesh_<n>_<ts>.args.txt` = argv completa + host_free + `FALLBACK whpx->tcg exit=…`
  se houver); **rotação de serial** (arquivo existente renomeado p/ `<ts>.txt` antes do
  Remove-Item — nome estável preservado p/ monitores); banner `[ACCEL]` ruidoso.
- `run-qemu-whpx.ps1`: `$LASTEXITCODE` após o launch alimenta a ladder retry (catch morto
  acionável); preflight (RamGB+1); args dump. `run-mesh4-lab.ps1`: gate 9GB.
- Validação dos scripts: `Parser::ParseFile` 0 erros nos 4; byte-scan >127 = 0 (11 linhas
  de comentário pré-existentes ASCII-izadas no run-qemu-whpx); dry-run do preflight
  (pass/refuse exit1/force-WARN) confirmado em child powershell.

## Evidência do fix (smoke decisivo)

`logs/boot_whpx_20261010_221534.txt` — `run-qemu-whpx.ps1 -RamGB 2` (1×2G, 4c, WHPX):
`hv=WHPX`; gate `firmware EXTD=1` **AUSENTE**; `LAPIC inicializado`;
`AP LAPIC 1..3 online (3/3)`; `CorePools r0=1 r1=2 r2=1 total=4`;
**`BOOT SCORE … smp_online=4`** (T+205). Args dump gravado
(`boot_whpx_20261010_221535.args.txt`, 1186B). QEMU morto pelo harness após o marco.

## Correções a claims anteriores

- S463 "M5 ZERO anomalias": verdade para OOM/PF/park, **mas** rodou BSP-only
  (`smp_online=1`) — o dispatch SMP não foi exercitado.
- Relato do orquestrador "fallback WHPX→TCG sob pressão de memória": **refutado**
  (TCG era o default do p2p-mesh; sem args dump não havia como ver).

## Pendentes / follow-ups

- Política (não bug): com EXTD=1 real (x2apic=on / HW moderno) o gate corrigido ainda
  desliga SMP fail-closed — suporte x2APIC real = futuro.
- Reprise M5/mesh4 com APs online para exercitar parallel_matmul sob os fixes s463.
- Lanes 6-7 (baseline bench + contrato e2e) seguem adiadas.

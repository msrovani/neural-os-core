# SESSION_469 — Survey 1.58b vira código: sampler + I2_S + 2B4T no rig (2/6)

Data: 2026-10-13. Continuação da saga Falcon3 .V7 (s468b/c/d). Pedido: sugestões de otimização baseadas em novidades 1.58b, lastreadas em HW real.

## Survey (@librarian, reconciliado)

- Nenhum modelo nativo aberto >2B; 2B4T segue canônico (MIT, ReLU², W1.58A8). Falcon-PTQ pior por byte.
- Ranking aceito: 1) I2_S sign-select scalar, 2) TL2 triple-LUT, 3) ReLU² gate-skip (só 2B4T), 4) acts INT8, 5) KV 2-bit KIVI, 6) packing 2bpw alinhado, 7) n-gram self-spec (~1.05-1.15x), 8) resto (treino-side) não.
- Caveat honesto: todos os speedups publicados usam SIMD; fator scalar não medido — nossa medida QEMU será o primeiro dado scalar.

## s468b — sampler com políticas do rig (commitado + pushed)

- `is_forbidden_id` (predicado, zero tabela — melhor que a spec), `PENALTY_LAST_N=64`, rep 1.15 ON default, semântica llama idêntica ao rig. 3 desvios justificados (eot isento, off-map fail-closed, recent-skip mantido). cortex 7/7.

## s468c — I2_S scalar (commitado + pushed)

- `i2s_ternary_gemv` (~50 linhas: absmax → int8 → sign-select int32 → 1 mul/linha) + funil `matmul_hybrid` com flag OFF default. Parity: exact vs quantizado (<1e-6), cos>0.999 vs FP + bound analítico. cortex 139/139. Ganho esperado vem de trocar FP emulado (soft-float) por ALU inteira, não de menos ops. Medida QEMU pendente.

## s468d — rig roda 2B4T (commitado + pushed)

- Arch real (NÃO Falcon): 30L/2560h/20Q/5KV/vocab 128256/inter 6912/ReLU²+SubLN/tied/theta 500000, tokenizer Llama-3, template próprio.
- Catch: .V7 inicial usava escala recíproca (convenção Falcon3) → parity 0.9542; reconversão direct-scale → 1.0000007. Diagnóstico recíproco 1.0000002 isolou o defeito a escala-only.
- Suite 2/6 (Q1+Q6) = TIE com Falcon3-1B; vermelhos por regra/persona, sem colapso; eos 0/7; Q3 com vazamento de peça especial.
- Vale wiring: ReLU² (~3 linhas) + 2 SubLN/layer + template BitNet. Despachado (reuse fix-1).

## Infra

- Primeiro dispatch do rig-extension reportou DNS-falha mas executou server-side (geração fantasma fez a extensão); resume recusado (owned); sessão fresca verificou em vez de duplicar. Lição em AGENTS.md.
- Sibling dirt intacta e fora dos commits: `hermes/agents.rs`, `k_nano/boot_ramlog.rs`.

## Pendente

Wiring kernel 2B4T → QEMU (tok/s I2_S + boot 2B4T) → decisão TL2 / ReLU-skip / KV-2bit.

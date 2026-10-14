# SESSION_475 — Próximos passos executados (QEMU 2B4T + 3 experimentos host)

Data: 2026-10-13. Execução dos 4 próximos passos recomendados.

## 1. Boot QEMU 2B4T (v7 loader validation)

**Comando**: `cargo build --release -p boot` + `build_image.py --hw --unified --build-boot --size 8192` + `run-qemu-whpx.ps1 -RamGB 4 -Smp 4 -Cpu Haswell`

**Resultado**: 
- ✅ Boot completo (8 fases + PostRuntime), SMP online=4, BOOT SCORE OK
- ✅ Falcon3-3B (989 MB) carrega do QEMU loader @0x100000000 — v6 header OK
- ❌ **BITNET2B4T.V7 (760 MB) REJEITADO** — `load_model REJECT header invalido h=0 L=0` no endereço 0x13DF00000
- **Causa**: v7 loader (s471) produz header format que o kernel v6 loader não reconhece
- **Solução**: Converter BITNET2B4T.V7 para v6 header OU estender `load_model_v6` para aceitar v7 header

**Log chave**:
```
[T+258] [R0] [nk] [BOOT] [ok] - === BOOT SCORE qemu=true ram_mb=5120 smp_online=4 ===
[T+259] [R0] [nk] [BOOT] [ok] - llm           ok
```

## 2. bench_kv_ternary.py (E09 — KV ternary recall)

**Modelo**: Falcon3-1B.V6, 500 tokens contexto, 36 perguntas needle-in-haystack

| Mode | Memory (MB) | vs fp16 | Exact Match | Avg F1 | Perplexity | Time |
|------|-------------|---------|-------------|--------|------------|------|
| fp16 | 35.16 | 100% | 0.0% | 0.000 | 46,464 | 1,233s |
| int8 | 17.58 | 50% | 0.0% | 0.000 | 46,464 | 1,041s |
| ternary | 17.58 | 50% | 0.0% | 0.000 | 46,464 | 1,007s |

**Conclusão**: **Inconclusivo** — modelo base falha na tarefa (0% EM em todos os modos). Gera lixo repetitivo (`0.net.net.net...`). 

**Problemas do benchmark**:
- Medição de memória errada: int8 e ternary mostram 17.58 MB (deveria ser ~4.4 MB para ternary = 12.5% fp16)
- Perplexidade idêntica sugere quantização não aplicada no forward
- Modelo não fine-tuned para long-context QA

**Próximo**: Usar modelo fine-tuned para QA / corrigir medição / validar quantização no forward.

## 3. --per-row-scales (E30) + suite Q1-Q6

**Conversão**: `convert_falcon3_to_v6.py --per-row-scales` → `FALCON3_1B_PERROW.V7` (726,856,228 bytes, byte-exact match)

**Suite Q1-Q6** (ambas versões):
| Teste | V7 normal | V7 per-row |
|-------|-----------|------------|
| Q1 | COERENTE | COERENTE |
| Q2 | DEGENERADO | DEGENERADO |
| Q3 | DEGENERADO | DEGENERADO |
| Q4 | DEGENERADO | DEGENERADO |
| Q5 | DEGENERADO | DEGENERADO |
| Q6 | COERENTE | COERENTE |

**Resultado**: **Zero mudança de coerência**. Escalas por linha no unembed (lm_head) não afetam qualidade neste modelo/quantização.

## 4. --draft-verify (E06) — Grammar-constrained draft-verify

**Testes**: 3 schemas JSON no Falcon3-1B

| Schema | Draft-Verify | Unconstrained Greedy |
|--------|--------------|---------------------|
| Simple object | 0 tok/s, 0% acceptance, invalid JSON | 1.31 tok/s, valid JSON ✓ |
| Array of objects | 0 tok/s, 0% acceptance, invalid JSON | 1.31 tok/s, valid JSON ✓ |
| Nested | 0 tok/s, 0% acceptance, invalid JSON | 1.36 tok/s, valid JSON ✓ |

**Root Cause**: `JSONSyntaxStateMachine` (chat_v6.py:793-1027) tem mismatches críticos com tokenizer Falcon3:
1. `string_content` só tem tokens com aspas — tokenizer produz tokens sem aspas
2. `whitespace` vazio — space=2226, newline=12, tab=13 não mapeados
3. Números multi-dígito quebram (transição após 1 dígito)
4. `--ask` ignorado (prompt hardcoded)
5. 0 tokens permitidos → todos rejeitados no verify

**Baseline unconstrained**: 100% JSON validity, 100% schema validity, ~1.3 tok/s

**Recomendação**: Rewrite completo do `JSONSyntaxStateMachine` — permitir qualquer token non-special em `in_string`, mapear whitespace do tokenizer, lidar com números multi-token, usar schema-aware constraints.

---

## Resumo Decisões

| Item | Status | Próxima Ação |
|------|--------|--------------|
| 2B4T boot | v7 header mismatch | Converter .V7 para v6 header OU estender load_model_v6 |
| E09 KV ternary | Inconclusivo (modelo errado) | Benchmark com modelo fine-tuned QA + fix medição |
| E30 per-row scales | Zero impacto | Não adotar por enquanto |
| E06 draft-verify | Prototype quebrado | Rewrite JSONSyntaxStateMachine |

## Pendentes (aguardam recursos)
- #702 TL2 LUT kernel → aguarda HW real I2_S AVX2 speedup
- #696 Piloto 1B-QAT → $200 ferro alugado
- #697 Inferência 7B/10B → ferro ≥16GB
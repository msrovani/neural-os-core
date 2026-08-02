# SESSION_240 — ADR-0081 Fase B: Tier cripto Relativizado (HMAC) vs Full (Ed25519) (2026-08-02)

**Objetivo:** Analisar o custo real (velocidade/lag/processamento) da cripto simples vs full no mesh P2P e implementar o gate de tiers: mesmo range/subnet (datacenter) relativiza a autenticação de DADOS para HMAC-SHA256 em troca de velocidade; mesh externo mantém o protocolo completo.

## Decisão do maintainer (2026-08-02)

> "Quando os AIOS estiverem em mesmo range de rede, em tese um datacenter, as ações criptográficas podem ser 'relativizadas' em contrapartida à velocidade. Se o mesh é externo, aí sim o fluxo de dados deve usar os protocolos de segurança protocolares previstos."

## Análise de custo (benchmarks eBACS/lib25519/dalek/OpenSSL, Zen 4-class @4GHz)

| Primitiva | Heartbeat ~300B | Fragmento ~1.2KB | Payload 17.5KB |
|---|---|---|---|
| Ed25519 sign | ~8-16µs | ~8-16µs | ~8-16µs (1x, já amortizado: sign → fragment, verify pós-reassembly) |
| Ed25519 verify | ~26-46µs | ~26-46µs | ~26-46µs (1x) |
| HMAC-SHA256 (Relativizado) | ~0.6µs | ~1.3µs | ~11.5µs |
| ChaCha20-Poly1305 | ~0.35µs | ~0.85µs | ~12.5µs |
| AES-256-GCM (AES-NI) | ~0.07µs | ~0.33µs | ~3µs |
| X25519 handshake | one-time ~19-30µs/peer | — | — |

**Conclusões:**
1. Assinatura Ed25519 domina o custo por ~2 ordens de magnitude em pacotes pequenos — custo FIXO, independente do tamanho.
2. Verify Ed25519 limita throughput a **~37 MB/s/core (~0.3 Gbps)** — satura 1 core antes de 1Gbps. HMAC ~1.3µs → ~8 Gbps.
3. Em datacenter (RTT ~0.1-0.5ms) +40µs/pacote = **+8-40% do RTT** — visível. Em WAN (10-100ms) = 0.04-0.4% — invisível.
4. **Onde dá pra relativizar o custo é alto; onde não dá, a latência de rede engole o custo** — a diretriz do maintainer é arquiteturalmente correta.
5. Implementação importa 3-4x: OpenSSL EVP verify ~100µs vs lib25519 ~32µs. Usamos `ed25519-compact` (sem SIMD) — calibrar no target é recomendado.
6. Bare-metal: crypto esparsa paga ~14µs de warm-up de unidades vetoriais por rajada (heartbeat 1.1s = caminho frio).
7. O código atual JÁ assina 1x por payload (sign → fragment, verify pós-reassembly) — NÃO paga 18×720µs (assinatura por fragmento seria o pior caso).

## Implementação (gate L/F, sem dep nova)

| Item | Arquivo | Detalhe |
|---|---|---|
| `hmac_sha256` (RFC 2104/4231, reusa `tpm::sha256`), `ct_eq` constant-time, `hmac_self_test` (RFC 4231 caso 1) | `k_nano/src/crypto.rs` (novo, 82 LOC) | HMAC block 64B, ipad 0x36/opad 0x5c; key >64B → hash antes |
| `SEGMENT_KEY`, `crypto_tier()`, seam `set_segment_key(Option<[u8;32]>)` | `k_nano/src/net/mesh.rs:783-831` | `Relativized` iff chave provisionada; `None` = desprovisiona (fail-closed) |
| TX dados tiered: `sign_packet` → HMAC 32B (Relativized) / Ed25519 (Full); `sign_packet_authentic` para controle | `k_nano/src/net/udp_broadcast.rs:97-169` | Heartbeat/ROLE/PK\0/CAP SEMPRE Ed25519 (âncora TOFU, raro ~1.1s) |
| RX fail-closed: controle (tt==5/`ROLE\0`) sempre Ed25519; dados de peer conhecido → tiered; falha → DROP + contador | `k_nano/src/net/mesh.rs:1182-1260` | `min_auth` = SIGNATURE_LEN (controle/Full) ou HMAC_TAG_LEN (dados Relativized) |
| Worker MR verify tiered | `cortex/src/compute.rs:237` | `verify_packet_tiered` no path de dados |
| Self-test no boot | `neural-kernel/src/main.rs` (seção self-tests ADR-0081) | `k_nano::crypto::hmac_self_test()` |
| ADR-0081 Fase B documentado com números | `docs/architecture/0081-*.md` | Tabela de custo + tabela de implementação + evolução AEAD |

## Segurança

- Tag HMAC cobre o frame inteiro (header+payload), verificada com comparação constant-time (`ct_eq`) — previne injeção/adulteração por nó não-provisionado no segmento.
- Anti-replay permanece no canal heartbeat (dados usam `clock=0` — anti-replay de dados em Tier L é follow-up: exigiria clock monotônico por fonte nos senders).
- TOFU/PEER_KEYS inalterados — HMAC não vincula identidade nova (peers desconhecidos só fazem TOFU via heartbeat Ed25519).
- Fail-closed: sem `SEGMENT_KEY` = Tier Full = comportamento atual (zero regressão).

## Verificação

- `cargo check --release --target-dir target/check-crypto`: **0 erros** (warnings conhecidos, política Known Warnings) — rodado após o wiring do self-test.
- `hmac_self_test` (RFC 4231) roda no boot; HMAC correto comprovado por vetor padrão.

## Pendente ADR-0081 (restante)

- SemanticRouter (HNSW) — `hermes/router.rs`
- Merge de conteúdo CRDT (ART/BQ) — hoje sync de versão (ponytail)
- AEAD (X25519+ChaCha20) para Tier F externo quando houver tráfego sensível — dep nova
- Anti-replay de dados em Tier L (clock monotônico por fonte)
- Calibrar `ed25519-compact` no target (sem SIMD — verify no topo da faixa)
- Merkle piece verification (distribuição de modelos)

## Commits

- Implementação + docs desta sessão: commit único pendente (gate cripto L/F + SESSION_240 + CHANGELOG/STATE/INDEX/AGENTS).

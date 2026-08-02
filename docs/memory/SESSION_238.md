# SESSION_238 — Segurança Fase A + Veredicto BitTorrent + Fragmentação MTU (ADR-0081)

**Data:** 2026-08-01
**Objetivo:** Fechar os pendentes de segurança e transporte do ADR-0081: Fase A TOFU (MITM), avaliar BitTorrent, e remover o gate MTU 1200B do matmul distribuído.

---

## 1. Segurança Fase A — TOFU + fail-closed + anti-replay ✅ (commit e56e5d4)

**Problema (MITM confirmado):** o caminho RX do mesh verificava com `session_public_key()` LOCAL contra assinatura do PEER → sempre falhava → caía no `None => rx` (fail-open: aceitava tudo). Sem tabela de peers, sem anti-replay, sem criptografia.

**Implementado:**
- RX fail-closed: pacote sem assinatura → DROP; assinatura inválida vs pk vinculada → DROP.
- TOFU: tabela `PEER_KEYS [(node_id, pk, last_clock); 16]`; heartbeat TX carrega `PK\0`+pk da sessão; 1º heartbeat de node desconhecido vincula (self-consistent, verificado contra a pk embutida); não-heartbeat de desconhecido → DROP.
- Anti-replay: heartbeats com `clock <= last` → DROP (LAN confiável; janela WAN = futuro).
- Todos os TX assinam: heartbeat, ROLE (k_nano), skill push/promote (hermes), offer (hermes), MW request + MR response (cortex — verifica MR contra `peer_public_key(sender)`).
- Contadores `sec: unsigned/badsig/replay` + log throttled.

**Validado QEMU dual: `sec: unsigned=0 badsig=0 replay=0`** (zero drops legítimos) + matmul distribuído OK.

## 2. Veredicto BitTorrent (ora-1 + lib-1) — registrado no ADR-0081 (commit e0fe270)

**Pergunta do maintainer:** torrent entra em que camada? Ajuda na segurança? Crate no_std? arXiv? Licença?

**Veredicto: NÃO implementar BitTorrent como protocolo** — é bulk-transfer WAN/anônimo/churn alto, o oposto do mesh (LAN, ≤16 nós, identidade explícita). Seria ~3-5k LOC para problema que HTTP Range + broadcast já cobrem.

| Pergunta | Resposta |
|---|---|
| Camada | Utilitário content-addressing na Transport R0 (só modelos/firmware Fase C + ADR-0046), não camada nova |
| Segurança | Ajuda 1 (merkle/infohash = integridade de conteúdo), atrapalha 2 (DHT sybil, MSE sem auth) — não substitui Fase A |
| Crate no_std | Nenhum completo; só `bendy` (bencode, BSD-3). Realista: wire BEP-3 + DHT BEP-5 próprios |
| arXiv 2024-26 | GenTorrent (2504.20101, KV-cache overlay), KDN (2409.13761), BasedAI (2403.01008), Petals |
| Licença | BEPs public domain → livres p/ AGPLv3; **uTP (BEP-29) patenteado até 19/11/2027 — evitar** |

**Subconjunto aproveitável:** merkle piece verification (~150 LOC, reusa `k_ai::merkle_audit`) quando modelos trafegarem. "O mesh quer o infohash, não o swarm."

**Nota SKYNET (trust seam):** `PEER_KEYS` é pré-preenchível via `peer_public_key()` — TEE attestation do SKYNET preenche a mesma tabela no futuro (TOFU=LAN, attestation=SKYNET global), zero mudança no mesh.

## 3. Fragmentação MTU + reassembly ✅ (commit 916d155)

**Problema:** gate `payload > 1200B → fallback local` limitava matmul grande e FL.

**Implementado (`FRAG\0` header de 21 bytes):**
- `send_fragmented(payload, port)`: ≤1200B → direto (compatibilidade total); >1200B → chunks ≤1000B, cada fragmento `FRAG\0` + frag_id u32 LE + total_frags u32 LE + frag_idx u32 LE + total_len u32 LE + dados. `frag_id` = AtomicU32 global.
- `recv_fragmented(port)`: pacote sem `FRAG\0` → retorna direto (heartbeat/ROLE/skills intactos); com prefixo → reassembly em 2 slots (fora-de-ordem OK, duplicatas via bitmask seen[8], timeout >500 TIMER_TICKS descarta slot) → completo concatena.
- Fragmentação DEPOIS de `sign_packet` (TX), reassembly ANTES de `verify_packet` (RX) — integridade preservada (Fase A intacta).
- `mesh.rs` p2p_tick: RX drain via `recv_fragmented` (antes o Master dropava `FRAG\0` do MW como unsigned).
- `compute.rs`: gate 1200B removido; MW/MR via `send_fragmented`/`recv_fragmented`; self-test 64×64 (exercita ~18 fragmentos).

**Validado QEMU dual (matmul 64×64 ~17.5KB):**
```
[B] frag TX id=3 partes=18 len=17528   (request fragmentado)
[A] frag RX id=1 partes=18 len=17528   (reassembled no Master)
[A] frag TX id=1 partes=17 len=16496   (resposta fragmentada)
[B] frag RX id=2 partes=17 len=16496   (reassembled no Worker)
[B] matmul resposta ok shape=(64,64) primeiro=2016.0 (mesh dispatch)
sec: unsigned=0 badsig=0 replay=0
```

## Lições

- **Fail-open é pior que não ter assinatura** — a assinatura cosmética (chave errada + fallback aceita) dá falsa segurança. Fail-closed primeiro, cripto depois.
- **TOFU self-consistent**: vincular a pk que o próprio pacote carrega (verificada contra si) é o TOFU honesto sem PKI; a tabela vira seam para TEE attestation futuro.
- **Fragmentação no transporte, não no chamador**: reassembly antes do verify preserva a cadeia de confiança sem tocar nos consumidores (EventBus/compute).
- **Antes de implementar protocolo novo (torrent)**: verificar se o problema já está resolvido por transporte existente (HTTP Range 1:1 + broadcast + fragmentação agora).

## Verificação

- `cargo check --release -p neural-kernel`: 0 erros (após cada edição).
- QEMU dual: mesh completo funciona com fail-closed (0 drops) + matmul 64×64 fragmentado.
- Commits: `e56e5d4` (Fase A), `e0fe270` (ADR BitTorrent), `916d155` (fragmentação).

## Pendente ADR-0081 (Fase C / próximos)

- Fase B cripto (X25519+ChaCha20) — só quando tráfego sensível em rede não-isolada
- CRDT #315.26, SKYNET #315.27, DSD, SemanticRouter, FedYogi, merkle piece verification

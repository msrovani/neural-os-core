# SESSION_153 — Release v1.9.0 TEST

**Data:** 2026-07-18  
**Versão:** **v1.9.0 TEST / NÃO ESTÁVEL**  
**Tag:** `v1.9.0`

## Escopo do marco

| Entrega | Evidência |
|---------|-----------|
| Residuals ondas 0–7 | SESSION_142–151; PreFlight |
| LAN L3.5–L5 e1000 | SESSION_149/150; TX `0x3800/0x3818` |
| Pós-LAN B-01 unlock | SESSION_152; `net_bridge`; NetFs PASS; TLS BLOCKED |
| Docs hygiene | TODO/STATE/IDEA/INDEX alinhados |

## O que NÃO é

- **Não** é v2.0.0 — gate permanece fechado (review ADR + AWAITING defer + OK maintainer)
- TLS real / WiFi RF / GPU golden / soft-float VITS — abertos conscientes

## Checklist release

- [x] CHANGELOG `[1.9.0]`
- [x] README / TODO / ROADMAP / SUMMARY / STATE / TECNOLOGIAS / HOWTO / AGENTS
- [x] `cargo check --release` 0 erros
- [x] tag `v1.9.0` + push

## Próximo

`/model-fetch` e2e · TLS `#123` · WiFi AWAITING · gate v2.0.0 review

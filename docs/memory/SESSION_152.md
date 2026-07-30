# SESSION_152 — Pós-LAN B-01 unlock (ondas 0–5)

**Data:** 2026-07-18  
**Escopo:** Destravar fila `depends_on: lan` / histórico B-01 após L3.5–L5 (SESSION_149/150).  
**Fora:** WiFi (`depends_on: wifi`), CRDT/SKYNET, SoftMAC, fake HTTPS.
📋 CRDT/SKYNET transferido para ADR-0081 (Malha Cognitiva Distribuída) — Fase C pós-gate

## Entregas

| Onda | Item | Resultado |
|------|------|-----------|
| 0 | `resolve_and_http_get` + Host header + shell netstat | ✅ bin `net.rs` / `netstack` |
| 0 | `hermes::net_bridge` registrado no boot | ✅ FE → NETSTACK real |
| 1 | `/fetch`, Browser, Search, RSS, Market, AutoLearn | ✅ DNS+HTTP; HTTPS → `tls_not_ready` |
| 1 | Email SMTP | ✅ residual honesto (`smtp_dialogue_unwired`) |
| 2 | AirLLM `/model-fetch` + Range/stream | ✅ DNS+hostname; `tools/serve_tiny_gguf.py` |
| 3 | NetFs #418 TCP gateway:4446 | ✅ `netfs.rs` + `tools/netfs_peer.py` + smoke |
| 4 | SelfUpdate #308 HTTP | ✅ `fetch_update` + FNV + slot A/B |
| 5 | TLS #123 | ✅ BLOCKED honesto (`softfloat_or_crate`); sem strip→:80 |

## Evidência build / PreFlight / QEMU

- `cargo build --release` → **0 erros**; imagem `uefi.img` regenerada
- Log: `logs/boot_postlan_152c_20260718_185051.txt`
  - L3.5/NET-HW `rx_alive` · L4 DNS · L5 HTTP 301
  - `[TLS] VERDICT=BLOCKED reason=softfloat_or_crate`
  - `[NETFS] VERDICT=PASS list=2 write/read ok` (peer `tools/netfs_peer.py`)
- Fix: hang pós-L5 = deadlock `smoke_if_online` sob `NETSTACK.lock` — removido
- PreFlight: `#418` promove a PASS/SKIP com marker NETFS; `tls-fetch` PARTIAL (sem HTTPS PASS); WiFi AWAITING

## Como smoke runtime (host)

```powershell
# Terminal 1 — NetFs peer
python tools\netfs_peer.py
# Terminal 2 — HTTP assets (GGUF tiny / UPDATE.MANIFEST)
python tools\serve_tiny_gguf.py
# Terminal 3
.\run-qemu-whpx.ps1
# Esperado serial: L3.5/L4/L5; [NETFS] VERDICT=PASS se peer up; [TLS] BLOCKED
```

Guest URLs: `http://10.0.2.2:8080/...` (slirp) · NetFs `10.0.2.2:4446`.

## Lições

1. Hermes FE **não** deve usar `hermes::net` espelho (NETSTACK vazio) — bridge obrigatório.
2. Search/RSS batendo em `dns_ip:80` era bug clássico pós-LAN.
3. `embedded-tls` + soft-float/`x86_64-unknown-none` → BLOCKED documentado, não fake HTTPS.
4. **Deadlock NETSTACK:** `smoke_if_online`/`tcp_exchange` **não** pode rodar com `NETSTACK` ainda locked em `bootstrap_early` (hang pós-L5). Smoke só em `main.rs` após return.

## Próximo

- Boot WHPX com `netfs_peer` + `serve_tiny_gguf` para promover PreFlight `#418` / `airllm-net` a PASS
- TLS real quando hardfloat/crate viável
- WiFi RF AWAITING_HW

# STATE — neural-os-core v1.9.99-s308 — Falcon3 GGUF inference wired (SESSION_309)
#   SESSION_309: Falcon3-3B-Instruct-1.58bit GGUF → CURRENT_MODEL no boot.
#   TQ2_0 (type 25) + BF16 dequant; tensor type IDs e metadata value IDs
#   corrigidos p/ spec GGUF padrão; GgufBackedModel auto-config (hidden/layers/
#   heads/kv_heads/vocab/rope_theta da metadata); chat <|system|>/<|user|>/<|assistant|>;
#   KV-cache 512; GGUF magic 0x46554747 no scan QEMU loader; 6/6 testes cortex PASS;
#   cargo check 0 erros; QEMU 4c GGUF LOADED + JARBAS greeting 40s (s307/s308 sem regressão).
#   SESSION_308: should_redistribute; inflight; IPI 0→1; Memory N≥5; Net ring3;
#   steal_burst half∩4; stats/64; HUD 32. Host 28 tests PASS.
#   SESSION_307: init_roles_from_pools(N); MAX_CORES=256 RQ; smp-runqueue default ON.
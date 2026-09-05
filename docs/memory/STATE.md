# STATE — neural-os-core v1.9.99-s309 — Falcon3 + HW image ready
#   SESSION_313: xHCI metal corrigido (RTSOFF+0x20, TRB IOC/CC, handoff,
#       CSZ/scratchpads/WPR); MSC runtime bloqueado; UI independente do timer;
#       agent tick offload gated até lock por-agent. Reteste Alienware pendente.
#   HW: target/usb_hw.img 6271MB (PACK_LLM=falcon3, --hw --unified --size 6144)
#       checklist: docs/memory/HW_FLASH_s309.md | HEAD e3b0075
#   SESSION_309: Falcon3 GGUF + QEMU 16c (max_aps 255); TTS+NSGDB.
#   SESSION_308: anti-churn RQ; Memory N≥5; steal_burst.
#   SESSION_307: roles ∝ N; MAX_CORES=256; smp-runqueue default ON.

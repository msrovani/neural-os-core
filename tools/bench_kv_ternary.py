#!/usr/bin/env python3
"""bench_kv_ternary.py — Needle-in-haystack recall benchmark for KV quantization.

Compares ternary KV (2-bit) vs INT8 KV (absmax) vs FP16 KV (baseline) on:
- Retrieval accuracy (exact match / F1)
- Perplexity on held-out context
- Memory usage (bytes)

Tests on Falcon3-1B (host CPU, fp32 reference).
"""
from __future__ import annotations
import argparse
import json
import os
import sys
import time
from pathlib import Path

import torch
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
from chat_v6 import V6, BPE, ensure_tokenizer, KV_QUANT, quantize_kv_ternary, quantize_kv_int8

# ── Needle-in-haystack test data ──────────────────────────────────────
# Long context with embedded "needles" (specific facts to retrieve)
CONTEXT_TEXT = """
The Neural OS Core project is a bare-metal operating system written in Rust with AI-native architecture.
The project uses a custom bootloader based on Limine UEFI and targets x86_64 architecture.
The kernel implements a capability-based security model with three logical rings: Ring 0 for HAL and drivers,
Ring 1 for device capabilities and hardware abstraction, and Ring 2 for agents and skills.
The system uses a Trinity MoE (Mixture of Experts) architecture with 7 experts: HwIdentify, HwControl,
RustCoder, DiskDiag, Security, Generator, and SpeechSynth. The router is trained with VOCAB=256 PT-BR tokens.
The Falcon3-1B model is used as the base LLM with 1.58-bit ternary quantization.
The project has 41 native agents including SystemAgent, MonitorAgent, HwBridgeAgent, NetAgent,
InputAgent, CortexAgent, HermesAgent, DisplayAgent, and others.
The boot sequence has 8 phases: SafeHarbor, MemoryCore, SystemBringup, Diagnostics,
HardwareDiscovery, DriverInit, AgentFleet, and Runtime.
The system uses a custom file system called NeuralFS with copy-on-write semantics.
Memory management uses a hybrid allocator: bump-pointer for boot, TALC for runtime.
The GPU compute dispatch uses NVIDIA PUSH_BUFFER via GPFIFO for compute shaders.
The audio pipeline uses Intel HDA with DMA capture at 48kHz 16-bit mono.
The mesh P2P network uses UDP broadcast with FRAG/FRACK reassembly and selective ACK.
The project uses WHPX acceleration on Windows with Haswell CPU emulation for AVX2 support.
The TTS system uses Piper with VITS architecture, 366 tensors, 15.6M parameters.
The STT system uses CTC with tiny LSTM, 55K parameters, 28 character vocabulary.
The HW Expert model identifies hardware from PCI/USB IDs with 44K unique devices.
The SelfHeal system detects missing firmware and skills, downloads them via HTTP.
The AutoLearn agent detects training needs, trains experts, and registers them.
The SleepCycle agent runs 5 phases: REPLAY, DREAM, CONSOLIDATE, PRUNE, REFLECT.
The Security agent runs 5 detectors with a pipeline for threat detection.
The Optimizer agent performs self-optimization with bounded retry and anti-loop.
The WakeWord agent detects "Jarvis" by energy pattern matching.
The HDA audio driver supports Intel HDA with 4 stream descriptors.
The USB driver uses xHCI with port scanning and device enumeration.
The network stack uses smoltcp with e1000 and virtio-net drivers.
The display uses double-buffered BGRA32 framebuffer with compositor.
The Orb UI renders a hexagonal grid with 5 glow layers and 24 deterministic particles.
The system targets 60 FPS with damage rects for efficient rendering.
The boot log is persisted to FAT32 partition on USB with 256KB pre-allocated.
The Tickv database provides persistent key-value storage with GC.
The NSGDB provides semantic memory with BM25 lexical recall and embedding similarity.
The project has 714 Rust files and ~214K lines of code across 12 workspace crates.
The K3CHJ workspace includes k_nano, k_hal, k_ai, cortex, hermes, jarbas crates.
The ADR-0088 establishes AIOS-first premissa: AI from boot, HITL decisions, auto-adapt.
The ADR-0113 establishes systemic optimization mission: stability, cognitive cycle proof,
memory persistence, observability, isolation, performance, complexity reduction.
The ADR-0081 defines mesh P2P with 16-slot reassembly, selective ACK, exponential backoff.
The ADR-0112 defines VRAM universal vendor-agnostic for W2A8 compute dispatch.
The ADR-0059 defines runtime app factory with wasmi WASM runtime (path A).
The ADR-0058 defines generative card desktop with embedded-graphics DrawTarget.
The ADR-0041 defines capability MVP with PoC P0-P9 for ring transitions.
The ADR-0036 defines JARVIS unified interaction layer with persona and voice.
The project uses soft-float ABI with no_std and no_main for bare-metal.
The heap is at 0x4000_0000_0000 with 512MB initial, talc allocator.
The framebuffer address is 0xB8000 + physical_memory_offset at runtime.
BitNet ternary uses ADD/SUB only, zero FPU in matmul, 2-bit packing (4 weights/byte).
The Trinity MoE router has hidden=64, vocab=256, routing telemetry counters.
The SDIO MoE has 95,812 entries from real .inf/.sys files with pefile analysis.
HardwareRegisterMap is generated by AI with 3 levels: HWID, family, heuristic.
WHPX with -cpu host executes AVX2 natively; only block AVX2 if TCG hypervisor.
The capability MVP includes AS+CR3+SPSC+Cap+int0x90, CapGate, FB, DMA/mmap, Ring3 iretq.
The project uses cargo check --release with 0 errors mandatory.
The build uses isolated target dirs: target/agent-<name>, target/check-<name>.
The bootloader 0.11.15 handles UEFI/BIOS handoff for kernel_main entry.
Active dependencies include spin 0.12.3, lazy_static 1.5.0, uart_16550 0.2.19,
x86_64 0.14.13, talc 4.4.3, libm 0.2, smoltcp 0.14, ed25519-compact 2.3.1.
"""

# Questions with expected answers (needles)
QUESTIONS = [
    ("What is the base LLM model used?", "Falcon3-1B"),
    ("How many native agents does the system have?", "41"),
    ("What is the VOCAB size for the Trinity MoE router?", "256"),
    ("What is the heap address?", "0x4000_0000_0000"),
    ("How many workspace crates are there?", "12"),
    ("What is the framebuffer address formula?", "0xB8000 + physical_memory_offset"),
    ("What quantization does BitNet use?", "1.58-bit ternary"),
    ("How many experts in Trinity MoE?", "7"),
    ("What is the TTS system?", "Piper with VITS"),
    ("What is the STT vocabulary size?", "28"),
    ("How many unique devices in HW Expert?", "44K"),
    ("What are the 5 SleepCycle phases?", "REPLAY, DREAM, CONSOLIDATE, PRUNE, REFLECT"),
    ("What acceleration does WHPX provide?", "AVX2 native execution"),
    ("What is the bootloader version?", "0.11.15"),
    ("What file system does the boot log use?", "FAT32"),
    ("How many Rust files in the project?", "714"),
    ("What is the K3CHJ workspace?", "k_nano, k_hal, k_ai, cortex, hermes, jarbas"),
    ("What does ADR-0088 establish?", "AIOS-first premissa"),
    ("What does ADR-0113 establish?", "systemic optimization mission"),
    ("What does ADR-0081 define?", "mesh P2P with 16-slot reassembly"),
    ("What allocator is used for runtime?", "talc"),
    ("What network stack is used?", "smoltcp"),
    ("What display format is used?", "BGRA32 double-buffered"),
    ("What is the Orb UI?", "hexagonal grid with 5 glow layers"),
    ("What is the target FPS?", "60"),
    ("How many lines of code?", "~214K"),
    ("What is the boot log size?", "256KB"),
    ("What database provides semantic memory?", "NSGDB"),
    ("What is the Tickv database?", "persistent key-value storage with GC"),
    ("What is the capability MVP?", "AS+CR3+SPSC+Cap+int0x90"),
]

def compute_f1(pred: str, expected: str) -> float:
    """Compute token-level F1 between prediction and expected answer."""
    pred_tokens = set(pred.lower().split())
    exp_tokens = set(expected.lower().split())
    if not pred_tokens or not exp_tokens:
        return 0.0
    intersection = pred_tokens & exp_tokens
    precision = len(intersection) / len(pred_tokens)
    recall = len(intersection) / len(exp_tokens)
    if precision + recall == 0:
        return 0.0
    return 2 * precision * recall / (precision + recall)


def exact_match(pred: str, expected: str) -> bool:
    """Check if expected answer appears in prediction (case-insensitive)."""
    return expected.lower() in pred.lower()


def measure_kv_memory(model: V6, kvc) -> int:
    """Measure KV cache memory usage in bytes."""
    total = 0
    for layer_kvc in kvc:
        k_cache, v_cache = layer_kvc[0], layer_kvc[1]
        if KV_QUANT == "ternary":
            # packed bytes + scale (float) + shape tuple
            total += k_cache[0].numel()  # packed bytes
            total += v_cache[0].numel()  # packed bytes
            total += 8 * 2  # two scales (float64)
        elif KV_QUANT == "int8":
            total += k_cache[0].numel()  # int8 elements
            total += v_cache[0].numel()  # int8 elements
            total += 8 * 2  # two scales
        else:
            total += k_cache.numel() * 2  # fp16 = 2 bytes
            total += v_cache.numel() * 2  # fp16 = 2 bytes
    return total


def run_benchmark(model_path: Path, kv_mode: str, max_context_tokens: int = 500,
                  max_gen_tokens: int = 64, seed: int = 42) -> dict:
    """Run needle-in-haystack benchmark for a specific KV quantization mode."""
    global KV_QUANT
    KV_QUANT = kv_mode
    print(f"\n{'='*60}")
    print(f"Benchmark: KV_QUANT={kv_mode}")
    print(f"{'='*60}")

    torch.manual_seed(seed)
    torch.set_num_threads(max(1, os.cpu_count() or 4))

    # Load model
    print(f"[load] {model_path} ...")
    m = V6(model_path)

    # Load tokenizer
    bpe = None
    if getattr(m, "tok_type", 0) == 1 and getattr(m, "tok_blob", b""):
        try:
            bpe = BPE.from_bpb1(m.tok_blob)
            print(f"[tok] embedded BPB1: vocab={len(bpe.vocab)}")
        except Exception as e:
            print(f"[tok] BPB1 failed ({e}) — falling back")
    if bpe is None:
        tp = ensure_tokenizer(model_path)
        if tp and tp.is_file():
            bpe = BPE(tp)
            print(f"[tok] external: vocab={len(bpe.vocab)}")
        else:
            print("[tok] no tokenizer — cannot run benchmark")
            return {}

    eos = bpe.eos
    fbd = None  # no forbidden for benchmark

    # Encode context
    context_ids = bpe.encode(CONTEXT_TEXT)
    context_ids = context_ids[:max_context_tokens]
    print(f"[ctx] context tokens: {len(context_ids)}")

    # Build prompt: context + question
    results = {
        "kv_mode": kv_mode,
        "context_tokens": len(context_ids),
        "questions": [],
        "memory_bytes": 0,
        "total_time": 0.0,
    }

    total_start = time.time()

    for i, (question, expected) in enumerate(QUESTIONS):
        prompt = f"Context:\n{CONTEXT_TEXT}\n\nQuestion: {question}\nAnswer:"
        prompt_ids = [bpe.bos] + bpe.encode(prompt)
        prompt_ids = [tid for tid in prompt_ids if tid < m.vocab] or [0]

        # Generate answer
        t0 = time.time()
        out_ids, _ = m.generate(prompt_ids, max_gen_tokens, eos,
                                temp=0.0, top_p=1.0, seed=seed + i,
                                repetition_penalty=1.0, forbidden_ids=fbd)
        dt = time.time() - t0

        answer = bpe.decode(out_ids).strip()
        em = exact_match(answer, expected)
        f1 = compute_f1(answer, expected)

        print(f"  Q{i+1}: {question}")
        print(f"    Expected: {expected}")
        print(f"    Got:      {answer[:100]}")
        print(f"    EM: {em}  F1: {f1:.3f}  time: {dt:.2f}s")

        results["questions"].append({
            "question": question,
            "expected": expected,
            "answer": answer,
            "exact_match": em,
            "f1": f1,
            "time": dt,
        })

    total_time = time.time() - total_start

    # Measure KV memory after full context
    # Re-run prefill to get final KV cache state
    kvc = [[torch.empty(1, 0, m.kv_heads, m.head_dim),
            torch.empty(1, 0, m.kv_heads, m.head_dim)]
           for _ in range(m.layers)]
    h = m.embed[:, context_ids].T.unsqueeze(0) * m.embed_scale
    for li, blk in enumerate(m.blocks):
        h = m.forward_layer(h, blk, kvc[li], 0)

    memory_bytes = measure_kv_memory(m, kvc)

    # Compute perplexity on held-out context (last 100 tokens of context)
    heldout = context_ids[-100:] if len(context_ids) > 100 else context_ids
    if len(heldout) > 1:
        # Simple perplexity: average negative log-likelihood
        kvc_perp = [[torch.empty(1, 0, m.kv_heads, m.head_dim),
                     torch.empty(1, 0, m.kv_heads, m.head_dim)]
                    for _ in range(m.layers)]
        h_perp = m.embed[:, context_ids[:-1]].T.unsqueeze(0) * m.embed_scale
        for li, blk in enumerate(m.blocks):
            h_perp = m.forward_layer(h_perp, blk, kvc_perp[li], 0)

        # Get logits for heldout tokens
        log_probs = []
        for tok in heldout:
            last = h_perp[:, -1:, :]
            U = m.unembed.float() if m.unembed is not None else m.embed
            s = m.unembed_scale
            logits = (m.rmsnorm(last, m.rms_final) @ U) * s
            probs = torch.softmax(logits[0, 0], dim=-1)
            log_probs.append(torch.log(probs[tok] + 1e-10).item())
            # Advance
            h_perp = m.embed[:, [tok]].T.unsqueeze(0) * m.embed_scale
            for li, blk in enumerate(m.blocks):
                h_perp = m.forward_layer(h_perp, blk, kvc_perp[li], 0)

        perplexity = float(np.exp(-np.mean(log_probs))) if log_probs else float('inf')
    else:
        perplexity = float('inf')

    # Aggregate metrics
    em_scores = [q["exact_match"] for q in results["questions"]]
    f1_scores = [q["f1"] for q in results["questions"]]

    results.update({
        "memory_bytes": memory_bytes,
        "memory_mb": memory_bytes / (1024 * 1024),
        "total_time": total_time,
        "avg_time_per_q": total_time / len(QUESTIONS),
        "exact_match_rate": sum(em_scores) / len(em_scores),
        "avg_f1": sum(f1_scores) / len(f1_scores),
        "perplexity": perplexity,
    })

    print(f"\n  Summary for {kv_mode}:")
    print(f"    Memory: {results['memory_mb']:.2f} MB")
    print(f"    Exact Match Rate: {results['exact_match_rate']:.1%}")
    print(f"    Avg F1: {results['avg_f1']:.3f}")
    print(f"    Perplexity: {perplexity:.2f}")
    print(f"    Total Time: {total_time:.2f}s")

    return results


def main():
    ap = argparse.ArgumentParser(description="KV Quantization Recall Benchmark")
    ap.add_argument("--model", type=str, default="target1/FALCON3_1B.V6")
    ap.add_argument("--max-context", type=int, default=500)
    ap.add_argument("--max-gen", type=int, default=64)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--modes", nargs="+", default=["fp16", "int8", "ternary"],
                    choices=["fp16", "int8", "ternary"])
    ap.add_argument("--output", type=str, default="bench_kv_results.json")
    args = ap.parse_args()

    model_path = Path(args.model)
    if not model_path.is_absolute():
        model_path = ROOT / model_path

    if not model_path.exists():
        sys.exit(f"Model not found: {model_path}")

    all_results = []
    for mode in args.modes:
        try:
            result = run_benchmark(model_path, mode, args.max_context, args.max_gen, args.seed)
            if result:
                all_results.append(result)
        except Exception as e:
            print(f"[ERROR] Mode {mode} failed: {e}")
            import traceback
            traceback.print_exc()

    # Save results
    output_path = ROOT / args.output
    with open(output_path, "w") as f:
        json.dump(all_results, f, indent=2)
    print(f"\n[done] Results saved to {output_path}")

    # Print comparison table
    if len(all_results) > 1:
        print("\n" + "="*80)
        print("COMPARISON TABLE")
        print("="*80)
        print(f"{'Mode':<10} {'Memory (MB)':<14} {'EM Rate':<10} {'Avg F1':<10} {'Perplexity':<12} {'Time (s)':<10}")
        print("-"*80)
        baseline_mem = all_results[0]["memory_bytes"]
        for r in all_results:
            mem_ratio = r["memory_bytes"] / baseline_mem if baseline_mem > 0 else 0
            print(f"{r['kv_mode']:<10} {r['memory_mb']:<14.2f} {r['exact_match_rate']:<10.1%} "
                  f"{r['avg_f1']:<10.3f} {r['perplexity']:<12.2f} {r['total_time']:<10.2f}")
            if r['kv_mode'] != 'fp16':
                print(f"           -> {mem_ratio:.2%} of baseline memory")


if __name__ == "__main__":
    main()
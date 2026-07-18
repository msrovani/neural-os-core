#!/usr/bin/env python3
"""prepare_extra_models.py â€” baixa, treina/converte e gera .bitnet para ModelHub.

Modelos:
  TinyStories-1M / ~15M   â†’ target/tinystories.bitnet (+ TINYSTOR.BIN)
  BitNet b1.58-large (~0.7Bâ‰ˆ850M) â†’ target/bitnet_850m.bitnet (+ BITNET850.BIN)
  BitNet b1.58-3B         â†’ target/bitnet_3B.bitnet (+ BITNET3B.BIN)
  RustCoder expert maior  â†’ target/rust_coder_2.bitnet (+ RUSTCDR2.BIN)

GPU: usa CUDA se disponÃ­vel (recomendado torch+cu118 na GTX 1050 / sm_61).
ConversÃ£o BitNet 3B faz streaming layer-a-layer (4GB VRAM).

Uso:
  python tools/prepare_extra_models.py --all
  python tools/prepare_extra_models.py --tiny --fast --pro --rustcoder
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import struct
import sys
from pathlib import Path

# Flush logs em tempo real (treino GPU longo)
try:
    sys.stdout.reconfigure(line_buffering=True)
    sys.stderr.reconfigure(line_buffering=True)
except Exception:
    pass

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
CACHE = TARGET / "hf_cache"
TARGET.mkdir(exist_ok=True)
CACHE.mkdir(exist_ok=True)

sys.path.insert(0, str(ROOT / "tools"))

# GTX 1050 (sm_61): precisa torch+cu118/cu126 — cu130 detecta GPU mas nao tem kernels.
os.environ.setdefault("CUDA_VISIBLE_DEVICES", "0")

try:
    import torch
    import torch.nn as nn
    import torch.nn.functional as F
    import torch.optim as optim
except ImportError:
    print("[FATAL] pip install torch")
    sys.exit(1)


def _cuda_kernels_ok() -> bool:
    if not torch.cuda.is_available():
        return False
    try:
        x = torch.zeros(8, device="cuda")
        _ = (x + 1).sum().item()
        del x
        torch.cuda.empty_cache()
        return True
    except Exception as e:
        print(f"[WARN] CUDA detectada mas kernels falharam: {e}")
        print("[HINT] pip install torch==2.13.0 --index-url https://download.pytorch.org/whl/cu126")
        return False


DEVICE = torch.device("cuda" if _cuda_kernels_ok() else "cpu")
print(f"[DEV] torch={torch.__version__} cuda_build={getattr(torch.version, 'cuda', None)} device={DEVICE}")
if DEVICE.type == "cuda":
    props = torch.cuda.get_device_properties(0)
    print(f"[GPU] {torch.cuda.get_device_name(0)} | VRAM={props.total_memory/1e9:.1f}GB | cc={props.major}.{props.minor}")
else:
    print("[WARN] GPU inutilizavel — use cu126 (sm_61). Treino CPU so com --allow-cpu")

MAGIC = 0xBE11BE11


def encode_trit_vec(kn: np.ndarray) -> bytes:
    flat = np.ascontiguousarray(kn, dtype=np.int8).reshape(-1)
    n = flat.size
    bits = np.zeros(n, dtype=np.uint8)
    bits[flat > 0] = 0b01
    bits[flat < 0] = 0b10
    pad = (-n) % 4
    if pad:
        bits = np.concatenate([bits, np.zeros(pad, dtype=np.uint8)])
    b = bits.reshape(-1, 4)
    packed = b[:, 0] | (b[:, 1] << 2) | (b[:, 2] << 4) | (b[:, 3] << 6)
    return packed.tobytes()


def absmean_quantize_t(t: torch.Tensor) -> np.ndarray:
    """GPU absmean â†’ int8 {-1,0,1}."""
    x = t.detach().to(DEVICE, dtype=torch.float32)
    scale = x.abs().mean() + 1e-6
    q = torch.round(x / scale).clamp(-1, 1).to(torch.int8).cpu().numpy()
    return q


def write_f32(f, arr: np.ndarray) -> None:
    f.write(np.ascontiguousarray(arr, dtype=np.float32).tobytes())


def write_header_v4(f, *, hidden, num_layers, num_heads, vocab, max_seq,
                    intermediate, num_kv, q_dim, tie, tok=b"CHAR:32-126", feat=0x03):
    f.write(struct.pack("<I", MAGIC))
    f.write(struct.pack("<H", 4))
    f.write(struct.pack("<I", 0))  # placeholder params
    f.write(struct.pack("<H", hidden))
    f.write(struct.pack("<H", num_layers))
    f.write(struct.pack("<H", num_heads))
    f.write(struct.pack("<I", vocab))
    f.write(struct.pack("<H", min(max_seq, 65535)))
    f.write(struct.pack("<H", intermediate))
    f.write(struct.pack("<H", num_kv))
    f.write(struct.pack("<H", q_dim))
    f.write(struct.pack("<I", 0))
    f.write(b"TIED" if tie else b"\x00\x00\x00\x00")
    f.write(struct.pack("B", 1))
    f.write(struct.pack("<I", len(tok)))
    f.write(tok)
    f.write(struct.pack("B", feat))


def fat_copy(src: Path, fat8: str) -> None:
    dst = TARGET / fat8
    shutil.copy2(src, dst)
    print(f"  [FAT] {src.name} â†’ {fat8} ({dst.stat().st_size/1024:.0f}KB)")


# â”€â”€â”€ TinyStories (HF â†’ .bitnet via GPU quant) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

def prepare_tinystories(repo: str = "roneneldan/TinyStories-1M", out_name: str = "tinystories.bitnet"):
    """Converte TinyStories-1M via state_dict (sem AutoModel — evita bug torchvision/cu126)."""
    print(f"\n=== TinyStories <- {repo} ===")
    from huggingface_hub import hf_hub_download

    local = CACHE / repo.replace("/", "__")
    local.mkdir(exist_ok=True, parents=True)
    print("  [DL] config + weights...")
    try:
        hf_hub_download(repo, "config.json", local_dir=str(local))
    except Exception as e:
        print(f"  [ERR] download config: {e}")
        return None

    weight_file = None
    for cand in ("model.safetensors", "pytorch_model.bin", "pytorch_model.pt"):
        try:
            weight_file = Path(hf_hub_download(repo, cand, local_dir=str(local)))
            break
        except Exception:
            continue
    if not weight_file:
        print("  [ERR] nenhum weight file")
        return None

    cfg = json.loads((local / "config.json").read_text(encoding="utf-8"))
    print(f"  [LOAD] {weight_file} on {DEVICE}")
    if weight_file.suffix == ".safetensors":
        from safetensors.torch import load_file
        state = load_file(str(weight_file), device="cpu")
    else:
        state = torch.load(str(weight_file), map_location="cpu", weights_only=True)

    def get(name: str) -> torch.Tensor:
        if name in state:
            return state[name]
        alt = name.replace("transformer.", "")
        if alt in state:
            return state[alt]
        raise KeyError(name)

    emb = get("transformer.wte.weight")
    hidden = int(emb.shape[1])
    vocab = int(emb.shape[0])
    num_layers = int(cfg.get("num_layers", cfg.get("n_layer", 8)))
    num_heads = int(cfg.get("num_heads", cfg.get("n_head", 16)))
    max_seq = int(cfg.get("max_position_embeddings", cfg.get("n_positions", 2048)))
    intermediate = hidden * 4
    q_dim = hidden
    print(f"  arch hidden={hidden} L={num_layers} heads={num_heads} vocab={vocab}")

    out = TARGET / out_name
    with open(out, "wb") as f:
        write_header_v4(
            f, hidden=hidden, num_layers=num_layers, num_heads=num_heads,
            vocab=vocab, max_seq=max_seq, intermediate=intermediate,
            num_kv=num_heads, q_dim=q_dim, tie=True, tok=b"tinystories_v1", feat=0x01,
        )
        emb_q = absmean_quantize_t(emb.to(DEVICE) if emb.numel() < 50_000_000 else emb)
        f.write(encode_trit_vec(np.ascontiguousarray(emb_q.T)))

        def pack_oi(oi: np.ndarray, out_d: int, in_d: int) -> bytes:
            if oi.shape == (out_d, in_d):
                pass
            elif oi.shape == (in_d, out_d):
                oi = oi.T
            else:
                oi = oi.reshape(out_d, in_d)
            return encode_trit_vec(np.ascontiguousarray(oi.T))

        for i in range(num_layers):
            pfx = f"transformer.h.{i}"
            ln1 = get(f"{pfx}.ln_1.weight")
            ln2 = get(f"{pfx}.ln_2.weight")
            write_f32(f, ln1.float().numpy())
            write_f32(f, ln2.float().numpy())
            write_f32(f, np.ones(hidden, dtype=np.float32))
            write_f32(f, np.ones(hidden, dtype=np.float32))

            # GPT-Neo: attn.attention.q/k/v_proj ou c_attn
            try:
                q = absmean_quantize_t(get(f"{pfx}.attn.attention.q_proj.weight").to(DEVICE))
                k = absmean_quantize_t(get(f"{pfx}.attn.attention.k_proj.weight").to(DEVICE))
                v = absmean_quantize_t(get(f"{pfx}.attn.attention.v_proj.weight").to(DEVICE))
                o = absmean_quantize_t(get(f"{pfx}.attn.attention.out_proj.weight").to(DEVICE))
            except KeyError:
                w = get(f"{pfx}.attn.attention.c_attn.weight")
                if w.shape[0] != 3 * hidden:
                    w = w.T
                q = absmean_quantize_t(w[:hidden].to(DEVICE))
                k = absmean_quantize_t(w[hidden:2*hidden].to(DEVICE))
                v = absmean_quantize_t(w[2*hidden:].to(DEVICE))
                o = absmean_quantize_t(get(f"{pfx}.attn.attention.c_proj.weight").to(DEVICE))

            try:
                fc = get(f"{pfx}.mlp.c_fc.weight")
                proj = get(f"{pfx}.mlp.c_proj.weight")
            except KeyError:
                fc = get(f"{pfx}.mlp.fc_in.weight")
                proj = get(f"{pfx}.mlp.fc_out.weight")
            if fc.shape[0] != intermediate:
                fc = fc.T
            if proj.shape[0] != hidden:
                proj = proj.T
            gate = absmean_quantize_t(fc.to(DEVICE))
            up = absmean_quantize_t(fc.to(DEVICE))
            down = absmean_quantize_t(proj.to(DEVICE))

            f.write(pack_oi(q, q_dim, hidden))
            f.write(pack_oi(k, q_dim, hidden))
            f.write(pack_oi(v, q_dim, hidden))
            f.write(pack_oi(o, hidden, q_dim))
            f.write(pack_oi(gate, intermediate, hidden))
            f.write(pack_oi(up, intermediate, hidden))
            f.write(pack_oi(down, hidden, intermediate))
            if i % 2 == 0:
                print(f"  [L] {i}/{num_layers}")
            if DEVICE.type == "cuda":
                torch.cuda.empty_cache()

        try:
            ln_f = get("transformer.ln_f.weight")
        except KeyError:
            ln_f = torch.ones(hidden)
        write_f32(f, ln_f.float().numpy())

    del state
    if DEVICE.type == "cuda":
        torch.cuda.empty_cache()
    print(f"  [OK] {out} ({out.stat().st_size/1024:.1f}KB)")
    fat_copy(out, "TINYSTOR.BIN")
    return out


def prepare_tinystories_15m():
    """Treina LM ~15M params no estilo TinyStories (GPU) se HF 15M ausente."""
    print("\n=== TinyStories-15M (treino GPU local) ===")
    # ~15M: hidden=384, L=6, heads=6, vocab=512, ffn=1536
    # params â‰ˆ vocab*h + L*(4*h*h + 2*h*ffn) â‰ˆ 512*384 + 6*(4*384^2 + 2*384*1536) â‰ˆ 0.2+6*(0.59+1.18) â‰ˆ 11M
    hidden, vocab, layers, heads, ffn = 384, 512, 8, 8, 1536
    seq = 64
    from train_models_gpu import BitNetLM, write_header, write_tensor, write_vec_f32

    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=layers,
                     num_heads=heads, ffn_dim=ffn).to(DEVICE)
    nparam = sum(p.numel() for p in model.parameters())
    print(f"  params={nparam:,} (~{nparam/1e6:.1f}M) device={DEVICE}")

    # Dataset sintÃ©tico estilo stories + opcional HF dataset sample
    stories = [
        b"Once upon a time there was a little girl who loved cats.",
        b"There was a boy named Tom. He found a magic stone.",
        b"The dog ran to the park and played with a ball.",
        b"Lily wanted to bake a cake for her mom. She smiled.",
        b"A small bird flew over the blue lake every morning.",
    ]
    try:
        from datasets import load_dataset
        print("  [DL] TinyStories dataset sample (streaming)...")
        ds = load_dataset("roneneldan/TinyStories", split="train", streaming=True)
        n = 0
        for row in ds:
            t = row.get("text") or ""
            if len(t) > 20:
                stories.append(t.encode("utf-8", errors="ignore")[:seq])
                n += 1
            if n >= 2000:
                break
        print(f"  [DL] {n} stories")
    except Exception as e:
        print(f"  [WARN] dataset skip: {e} - usando seeds")

    def tok(b: bytes):
        arr = [(x % (vocab - 1)) + 1 for x in b[:seq]]
        arr += [0] * (seq - len(arr))
        return arr

    data = torch.tensor([tok(s if isinstance(s, bytes) else s) for s in stories], device=DEVICE)
    # next-token: shift
    x = data[:, :-1]
    y = data[:, 1:]
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(x, y), batch_size=16, shuffle=True)

    opt = optim.AdamW(model.parameters(), lr=2e-3)
    epochs = 40 if DEVICE.type == "cuda" else 15
    best = 1e9
    out = TARGET / "tinystories_15m.bitnet"
    for ep in range(epochs):
        model.train()
        tot = 0.0
        n = 0
        for xb, yb in loader:
            opt.zero_grad()
            # BitNetLM forward expects full seq â€” pad last
            pad = torch.zeros((xb.size(0), 1), dtype=torch.long, device=DEVICE)
            logits = model(torch.cat([xb, pad], dim=1))[:, :-1]
            loss = F.cross_entropy(logits.reshape(-1, vocab), yb.reshape(-1))
            loss.backward()
            opt.step()
            tot += loss.item()
            n += 1
        avg = tot / max(n, 1)
        if avg < best:
            best = avg
            model.export_bitnet(out, tok_data=b"tinystories_15m")
        if (ep + 1) % 5 == 0 or ep == 0:
            print(f"  epoch {ep+1}/{epochs} loss={avg:.4f} best={best:.4f}")

    model.export_bitnet(out, tok_data=b"tinystories_15m")
    # Prefer 15M as TINYSTOR if 1M failed size; also copy TINY.BIN
    fat_copy(out, "TINY.BIN")
    print(f"  [OK] {out} loss={best:.4f}")
    return out


# â”€â”€â”€ BitNet HF float16 (1bitLLM) â†’ .bitnet â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

def prepare_bitnet_hf(repo: str, out_name: str, fat8: str):
    print(f"\n=== BitNet â† {repo} â†’ {out_name} ===")
    from huggingface_hub import snapshot_download
    from safetensors.torch import load_file

    local = CACHE / repo.replace("/", "__")
    print("  [DL] snapshot (pode demorar)â€¦")
    snapshot_download(repo, local_dir=str(local), local_dir_use_symlinks=False,
                      ignore_patterns=["*.md", "*.txt", "flax*", "tf*"])

    cfg = json.loads((local / "config.json").read_text(encoding="utf-8"))
    hidden = int(cfg["hidden_size"])
    num_layers = int(cfg["num_hidden_layers"])
    num_heads = int(cfg["num_attention_heads"])
    num_kv = int(cfg.get("num_key_value_heads", num_heads))
    vocab = int(cfg["vocab_size"])
    max_seq = int(cfg.get("max_position_embeddings", 2048))
    intermediate = int(cfg["intermediate_size"])
    tie = bool(cfg.get("tie_word_embeddings", True))
    q_dim = hidden  # MHA full
    head_dim = hidden // num_heads

    st_path = local / "model.safetensors"
    if not st_path.exists():
        # shards
        shards = sorted(local.glob("model-*.safetensors"))
        if not shards:
            shards = sorted(local.glob("*.safetensors"))
        if not shards:
            print("  [ERR] sem safetensors")
            return None
        print(f"  [LOAD] {len(shards)} shards (streaming)")
        state = {}
        for sh in shards:
            part = load_file(str(sh), device="cpu")
            state.update(part)
            del part
    else:
        print(f"  [LOAD] {st_path} ({st_path.stat().st_size/1e9:.2f}GB)")
        state = load_file(str(st_path), device="cpu")

    out = TARGET / out_name
    print(f"  hidden={hidden} L={num_layers} H={num_heads} kv={num_kv} ffn={intermediate}")

    def get(name: str) -> torch.Tensor:
        t = state[name]
        if not isinstance(t, torch.Tensor):
            t = torch.tensor(t)
        return t

    with open(out, "wb") as f:
        write_header_v4(
            f, hidden=hidden, num_layers=num_layers, num_heads=num_heads,
            vocab=vocab, max_seq=max_seq, intermediate=intermediate,
            num_kv=num_kv, q_dim=q_dim, tie=tie, tok=f"bitnet_{repo.split('/')[-1]}".encode()[:32],
            feat=0x03,
        )
        emb = get("model.embed_tokens.weight")
        emb_q = absmean_quantize_t(emb.to(DEVICE) if emb.numel() < 80_000_000 else emb)
        # (vocab, hidden) â†’ packed (hidden, vocab)
        f.write(encode_trit_vec(np.ascontiguousarray(emb_q.T)))
        print(f"  [T] embed done")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # norms
            for nkey in (
                f"{p}.input_layernorm.weight",
                f"{p}.post_attention_layernorm.weight",
            ):
                write_f32(f, get(nkey).float().numpy())
            # optional sub norms â€” ones if absent
            for nkey in (f"{p}.self_attn.attn_sub_norm.weight", f"{p}.mlp.ffn_sub_norm.weight"):
                if nkey in state:
                    write_f32(f, get(nkey).float().numpy())
                else:
                    write_f32(f, np.ones(hidden, dtype=np.float32))

            def proj(name: str, out_d: int, in_d: int) -> bytes:
                w = get(name)
                # Linear: (out, in)
                if w.ndim != 2:
                    raise ValueError(name)
                if tuple(w.shape) == (in_d, out_d):
                    w = w.T
                # move to GPU for quant if fits
                if w.numel() * 4 < 500_000_000 and DEVICE.type == "cuda":
                    q = absmean_quantize_t(w.to(DEVICE))
                else:
                    q = absmean_quantize_t(w.float())
                if q.shape != (out_d, in_d):
                    q = q.reshape(out_d, in_d)
                return encode_trit_vec(np.ascontiguousarray(q.T))

            k_dim = num_kv * head_dim
            f.write(proj(f"{p}.self_attn.q_proj.weight", q_dim, hidden))
            f.write(proj(f"{p}.self_attn.k_proj.weight", k_dim, hidden))
            f.write(proj(f"{p}.self_attn.v_proj.weight", k_dim, hidden))
            f.write(proj(f"{p}.self_attn.o_proj.weight", hidden, q_dim))
            f.write(proj(f"{p}.mlp.gate_proj.weight", intermediate, hidden))
            f.write(proj(f"{p}.mlp.up_proj.weight", intermediate, hidden))
            f.write(proj(f"{p}.mlp.down_proj.weight", hidden, intermediate))
            if DEVICE.type == "cuda":
                torch.cuda.empty_cache()
            if li % 2 == 0 or li + 1 == num_layers:
                print(f"  [L] {li+1}/{num_layers} size={f.tell()/1e6:.1f}MB")

        write_f32(f, get("model.norm.weight").float().numpy())

    del state
    print(f"  [OK] {out} ({out.stat().st_size/1e6:.1f}MB)")
    fat_copy(out, fat8)
    return out


# â”€â”€â”€ RustCoder maior (GPU) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

def prepare_rustcoder2(epochs: int = 80):
    print("\n=== RustCoder-2 (expert maior, GPU) ===")
    # ~2â€“4M params â€” cabe na 1050; â€œ2Bâ€ pleno exige >24GB. Nome RUSTCDR2 = geracao 2.
    if DEVICE.type == "cuda":
        hidden, layers, heads, ffn = 256, 10, 8, 1024
    else:
        hidden, layers, heads, ffn = 128, 6, 8, 256
    vocab, seq = 128, 128
    from train_models_gpu import BitNetLM, _load_rusttraining_pairs, _build_seq_bytes

    model = BitNetLM(hidden=hidden, vocab=vocab, num_layers=layers,
                     num_heads=heads, ffn_dim=ffn).to(DEVICE)
    print(f"  params={sum(p.numel() for p in model.parameters()):,} device={DEVICE}")

    pairs = _load_rusttraining_pairs()
    tokens, targets = [], []
    for inp, out in pairs:
        tokens.append(_build_seq_bytes(inp, vocab, seq))
        targets.append(_build_seq_bytes(out, vocab, seq))
    xt = torch.tensor(tokens, device=DEVICE)
    yt = torch.tensor(targets, device=DEVICE)
    loader = torch.utils.data.DataLoader(
        torch.utils.data.TensorDataset(xt, yt), batch_size=8, shuffle=True)

    opt = optim.AdamW(model.parameters(), lr=2e-4)
    out = TARGET / "rust_coder_2.bitnet"
    best = 1e9
    for ep in range(epochs):
        model.train()
        tot = 0.0
        n = 0
        for x, y in loader:
            opt.zero_grad()
            logits = model(x)
            loss = F.cross_entropy(logits.view(-1, vocab), y.view(-1))
            loss.backward()
            opt.step()
            tot += loss.item()
            n += 1
        avg = tot / max(n, 1)
        if avg < best:
            best = avg
            model.export_bitnet(out, tok_data=b"rustcoder_gen2")
        if (ep + 1) % 10 == 0 or ep == 0:
            print(f"  epoch {ep+1}/{epochs} loss={avg:.4f} best={best:.4f}")
    model.export_bitnet(out, tok_data=b"rustcoder_gen2")
    fat_copy(out, "RUSTCDR2.BIN")
    # also refresh legacy name if newer
    fat_copy(out, "RUSTCDR.BITNET")
    print(f"  [OK] {out}")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--tiny", action="store_true", help="TinyStories-1M HF")
    ap.add_argument("--tiny15", action="store_true", help="TinyStories ~15M train")
    ap.add_argument("--fast", action="store_true", help="BitNet large ~850M")
    ap.add_argument("--pro", action="store_true", help="BitNet 3B")
    ap.add_argument("--rustcoder", action="store_true")
    ap.add_argument("--epochs", type=int, default=80)
    ap.add_argument("--allow-cpu", action="store_true",
                    help="Permite treino/quant em CPU (padrao: exige GPU com kernels)")
    args = ap.parse_args()
    if not any([args.all, args.tiny, args.tiny15, args.fast, args.pro, args.rustcoder]):
        args.all = True

    needs_train = args.all or args.tiny15 or args.rustcoder
    if needs_train and DEVICE.type != "cuda" and not args.allow_cpu:
        print("[FATAL] Treino exige GPU (sm_61). Instale torch+cu126 e CUDA_VISIBLE_DEVICES=0")
        print("  pip install torch==2.13.0+cu126 --index-url https://download.pytorch.org/whl/cu126")
        sys.exit(2)

    if args.all or args.tiny:
        try:
            prepare_tinystories("roneneldan/TinyStories-1M", "tinystories.bitnet")
        except Exception as e:
            print(f"[ERR] tinystories-1m: {e}")
            import traceback
            traceback.print_exc()
    if args.all or args.tiny15:
        try:
            prepare_tinystories_15m()
        except Exception as e:
            print(f"[ERR] tinystories-15m: {e}")
            import traceback
            traceback.print_exc()
    if args.all or args.fast:
        try:
            prepare_bitnet_hf("1bitLLM/bitnet_b1_58-large", "bitnet_850m.bitnet", "BITNET850.BIN")
        except Exception as e:
            print(f"[ERR] bitnet-850: {e}")
            import traceback
            traceback.print_exc()
    if args.all or args.pro:
        try:
            prepare_bitnet_hf("1bitLLM/bitnet_b1_58-3B", "bitnet_3B.bitnet", "BITNET3B.BIN")
        except Exception as e:
            print(f"[ERR] bitnet-3b: {e}")
            import traceback
            traceback.print_exc()
    if args.all or args.rustcoder:
        try:
            prepare_rustcoder2(epochs=args.epochs)
        except Exception as e:
            print(f"[ERR] rustcoder2: {e}")
            import traceback
            traceback.print_exc()

    # Alias MICRO / fast
    if (TARGET / "bitnet_850m.bitnet").exists() and not (TARGET / "MICRO.BITNET").exists():
        fat_copy(TARGET / "bitnet_850m.bitnet", "MICRO.BIN")

    print("\n=== RESUMO target/ ===")
    for p in sorted(TARGET.glob("*.bitnet")) + sorted(TARGET.glob("*.BIN")) + sorted(TARGET.glob("*.BITNET")):
        if p.is_file():
            print(f"  {p.name:28s} {p.stat().st_size/1e6:8.2f} MB")


if __name__ == "__main__":
    main()


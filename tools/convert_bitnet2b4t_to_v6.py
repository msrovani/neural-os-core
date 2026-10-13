#!/usr/bin/env python3
"""Converte microsoft/bitnet-b1.58-2B-4T -> .V7 BitNet (IDEA #691 / ADR-0116 E16).

Arquitetura LIDA de config.json — NAO assume shapes Falcon3 (nao e Falcon):
  hidden=2560 L=30 heads=20 kv=5 vocab=128256 inter=6912 act=relu2
  norm=SubLN (attn_sub_norm + ffn_sub_norm) tie=true sem bias.

Nativo 1.58bit: packed u8 HF -> desembrulha + transpoe via unpack_hf_oi
(reuso convert_falcon3_to_v6, unpack_conv=1 HF-strided, paridade exata vs
referencia HF provada inline). ESCALA DIRETA do buffer (scale_conv=0):
sonda de magnitude vs o forward de referencia (AutoBitLinear offline:
ActQuant8(x) @ signs * buf) provou rms(rig)/rms(ref)=1.0000 com buf as-is,
enquanto o reciproco (convencao Falcon-nativa) erra por buf^2 (0.21-0.67).
Os exports diferem: BitNet-2B4T grava weight_scale=mean|w|-like e o forward
de referencia MULTIPLICA. Direcao derivada do forward, nunca do nome.
Embed denso BF16 -> Q6_K (tied: sem unembed).

Reusa bitnet_writer.py (v7 = v6 + provenance + tokenizer BPB1 embutido).
Nao duplica o writer; nao toca em convert_falcon3_to_v6.py nem em crates/.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import struct
import sys
from pathlib import Path

import numpy as np

try:
    from tools.bitnet_writer import (
        write_header_v7, write_embed, write_rms, write_ternary, write_q6k,
        compute_feat, MODEL_LLM, ACT_RELU2, ACT_SILU, EMBED_Q6K,
    )
    from tools.convert_falcon3_to_v6 import unpack_hf_oi, load_state_dict
    from tools.export_bpe_bin import export as bpe_export
except ImportError:
    from bitnet_writer import (
        write_header_v7, write_embed, write_rms, write_ternary, write_q6k,
        compute_feat, MODEL_LLM, ACT_RELU2, ACT_SILU, EMBED_Q6K,
    )
    from convert_falcon3_to_v6 import unpack_hf_oi, load_state_dict
    from export_bpe_bin import export as bpe_export

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SRC = ROOT / "target1" / "bitnet2b4t"
DEFAULT_OUT = ROOT / "target1" / "BITNET2B4T.V7"
PROV_REPO = "microsoft/bitnet-b1.58-2B-4T"


def get_native_direct(state: dict, key: str, rows: int, cols: int):
    """Tensor nativo 1.58bit -> (int8 (rows,cols), scale direta do buffer).

    Unpack HF-strided + transpose (mesma orientacao do .V7: (in,out));
    escala = weight_scale as-is (forward de referencia MULTIPLICA — provado
    por sonda de magnitude; SEM reciproco). Falha alto se ausente.
    """
    arr = state.get(key)
    if arr is None or not isinstance(arr, np.ndarray) or arr.dtype != np.uint8:
        raise SystemExit(f"{key}: esperado packed u8 nativo, veio "
                         f"{None if arr is None else getattr(arr, 'dtype', type(arr))}")
    if arr.ndim != 2 or arr.shape[0] * 4 != cols or arr.shape[1] != rows:
        raise SystemExit(f"{key}: shape packed {arr.shape} != ({cols // 4},{rows})")
    q = unpack_hf_oi(arr).astype(np.int8).T.copy()
    assert q.shape == (rows, cols), (key, q.shape, rows, cols)
    skey = key.replace(".weight", ".weight_scale")
    s = state.get(skey)
    if s is None:
        raise SystemExit(f"{skey}: weight_scale ausente (sem palpite)")
    return q, float(np.asarray(s).flat[0])


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for blk in iter(lambda: f.read(1 << 20), b""):
            h.update(blk)
    return h.hexdigest()


def convert(source: Path, output: Path, repo: str = PROV_REPO) -> None:
    state, cfg = load_state_dict(source)

    hidden = int(cfg["hidden_size"])
    num_layers = int(cfg["num_hidden_layers"])
    num_heads = int(cfg["num_attention_heads"])
    vocab_size = int(cfg["vocab_size"])
    max_seq = int(cfg.get("max_position_embeddings", 4096))
    intermediate_size = int(cfg["intermediate_size"])
    num_kv_heads = int(cfg.get("num_key_value_heads", num_heads))
    tie = bool(cfg.get("tie_word_embeddings", False))
    rope_theta = float(cfg.get("rope_theta", 500000.0))
    act_name = str(cfg.get("hidden_act", "relu2")).lower()
    if act_name in ("relu2", "relu_2", "relu-squared"):
        act_type = ACT_RELU2
    elif act_name == "silu":
        act_type = ACT_SILU
    else:
        raise SystemExit(f"hidden_act desconhecido: {act_name!r} (sem palpite)")

    head_dim = hidden // num_heads
    q_dim = hidden
    k_dim = num_kv_heads * head_dim
    ffn_group = intermediate_size * q_dim // hidden

    print(f"  hidden={hidden} L={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  q_dim={q_dim} k_dim={k_dim} ffn_group={ffn_group} tie={tie}")
    print(f"  vocab={vocab_size} intermediate={intermediate_size} "
          f"theta={rope_theta:g} act={act_name}")

    # feat detectado das chaves (SubLN real, nao hardcoded).
    has_inner = any("attn_sub_norm" in k for k in state.keys())
    has_ffn = any("ffn_sub_norm" in k for k in state.keys())
    has_theta = True
    feat = compute_feat(has_inner, has_ffn, has_theta)
    print(f"  subln: attn_sub_norm={has_inner} ffn_sub_norm={has_ffn} "
          f"-> feat=0x{feat:02x}")

    # v7: provenance + tokenizer embutido (BPB1 modo llama: Llama-3 128k).
    import subprocess as _sp
    try:
        _git = _sp.run(["git", "rev-parse", "HEAD"], capture_output=True,
                       text=True, cwd=str(ROOT)).stdout.strip()
    except Exception:
        _git = ""
    prov = {"repo": repo, "revision": "main",
            "conv": "convert_bitnet2b4t_to_v6.py native-direct-scale+Q6K-embed(tied)",
            "git": _git or "unknown",
            "scale_conv": 0, "unpack_conv": 1}
    tok_bpb1 = b""
    _tok_json = source / "tokenizer.json"
    if _tok_json.is_file():
        import tempfile as _tf
        with _tf.NamedTemporaryFile(suffix=".bin", delete=False) as _fh:
            _tmp = Path(_fh.name)
        try:
            _rc = bpe_export(_tok_json, _tmp, sp32=False)
            if _rc == 0:
                tok_bpb1 = _tmp.read_bytes()
                print(f"  [T] tok embutido {len(tok_bpb1) // 1024}KB <- {_tok_json.name}")
            else:
                print(f"  [WARN] export BPB1 rc={_rc} — tok vazio")
        finally:
            _tmp.unlink(missing_ok=True)
    else:
        print(f"  [WARN] {_tok_json} ausente — tok vazio")

    num_params = hidden * vocab_size  # embed
    if has_ffn:
        num_params += intermediate_size * num_layers  # rms_ffn_norm
    if has_inner:
        num_params += hidden * num_layers  # rms_inner_attn
    per_layer = (hidden * q_dim + hidden * k_dim * 2 + q_dim * hidden
                 + hidden * ffn_group * 2 + intermediate_size * q_dim)
    num_params += per_layer * num_layers
    if not tie:
        num_params += hidden * vocab_size  # unembed (nao e o caso: tied)

    output.parent.mkdir(parents=True, exist_ok=True)
    with open(output, "wb") as f:
        write_header_v7(
            f, model_type=MODEL_LLM, num_params=num_params,
            hidden=hidden, layers=num_layers, heads=num_heads,
            vocab=vocab_size, max_seq=min(max_seq, 65535),
            intermediate=intermediate_size, kv_heads=num_kv_heads,
            q_dim=q_dim, medusa=0, tie=tie, tok_bpb1=tok_bpb1,
            act_type=act_type, embed_type=EMBED_Q6K, feat=feat,
            provenance=prov,
        )
        # embed (vocab, hidden) BF16 denso -> Q6_K (hidden, vocab) row-major.
        emb_key = next((k for k in state if "embed_tokens" in k), None)
        if emb_key is None:
            raise SystemExit("embed_tokens ausente no checkpoint")
        emb = np.asarray(state[emb_key], dtype=np.float32)
        if emb.shape == (hidden, vocab_size):
            emb = emb.T
        assert emb.shape == (vocab_size, hidden), f"embed shape {emb.shape}"
        write_embed(f, np.ascontiguousarray(emb.T), EMBED_Q6K, scale=1.0)
        print(f"  [T] embed {emb.shape} -> Q6_K "
              f"{((hidden * vocab_size + 255) // 256) * 210} bytes")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            for rk in [f"{p}.input_layernorm.weight",
                       f"{p}.post_attention_layernorm.weight"]:
                arr = np.asarray(state.get(rk, np.ones(hidden, dtype=np.float32)),
                                 dtype=np.float32).reshape(-1)
                assert arr.size == hidden, f"{rk} size {arr.size}"
                write_rms(f, arr)
            if has_inner:
                rk = f"{p}.self_attn.attn_sub_norm.weight"
                write_rms(f, np.asarray(state[rk], dtype=np.float32).reshape(-1))
            if has_ffn:
                rk = f"{p}.mlp.ffn_sub_norm.weight"
                arr = np.asarray(state[rk], dtype=np.float32).reshape(-1)
                assert arr.size == intermediate_size, f"{rk} size {arr.size}"
                write_rms(f, arr)

            for key, rows, cols in [
                (f"{p}.self_attn.q_proj.weight", hidden, q_dim),
                (f"{p}.self_attn.k_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.v_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.o_proj.weight", q_dim, hidden),
                (f"{p}.mlp.gate_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.up_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.down_proj.weight", intermediate_size, q_dim),
            ]:
                q, scale = get_native_direct(state, key, rows, cols)
                write_ternary(f, q.ravel(), scale)

            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell() // 1024}KB")

        rk = "model.norm.weight"
        write_rms(f, np.asarray(state.get(rk, np.ones(hidden, dtype=np.float32)),
                                dtype=np.float32).reshape(-1))
        if not tie:
            raise SystemExit("checkpoint nao-tied: unembed denso nao previsto")
        if has_theta:
            f.write(struct.pack("<f", rope_theta))

    sz = os.path.getsize(output)
    print(f"\n[OK] {output}: {sz:,} bytes ({sz / 1024 / 1024:.1f} MB)")
    print(f"  v7: act=RELU2 embed=Q6_K feat=0x{feat:02x} tie={tie} theta={rope_theta:g}")
    for cand in [source / "model.safetensors", source / "tokenizer.json"]:
        if cand.is_file():
            print(f"  [H] sha256({cand.name})={sha256_file(cand)}")
    print(f"  [H] sha256({output.name})={sha256_file(output)}")


def main():
    ap = argparse.ArgumentParser(description="bitnet-b1.58-2B-4T -> .V7 BitNet")
    ap.add_argument("--source", type=Path, default=DEFAULT_SRC)
    ap.add_argument("--output", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--repo", type=str, default=PROV_REPO)
    args = ap.parse_args()
    convert(args.source, args.output, repo=args.repo)


if __name__ == "__main__":
    main()

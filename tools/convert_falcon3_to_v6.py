#!/usr/bin/env python3
"""Converte Falcon3 3B (tiiuae/Falcon3-3B-Base) -> .bitnet v6 (ADR-0085).

Suporta dois modos:
  (a) denso BF16 ternarizado (threshold media abs, pack 4/byte)
  (b) nativo 1.58bit (tiiuae/Falcon3-3B-Instruct-1.58bit, ja ternario)

Reusa bitnet_writer.py: write_header_v6, pack_ternary, _encode_q6k, compute_feat.
"""
from __future__ import annotations
import argparse
import json
import os
import struct
import sys
from pathlib import Path

import numpy as np

try:
    from tools.bitnet_writer import (
        write_header_v6, write_header_v7, write_embed, write_rms, write_ternary, write_q6k, compute_feat,
        MODEL_LLM, ACT_SILU, EMBED_Q6K,
    )
except ImportError:
    from bitnet_writer import (
        write_header_v6, write_header_v7, write_embed, write_rms, write_ternary, write_q6k, compute_feat,
        MODEL_LLM, ACT_SILU, EMBED_Q6K,
    )

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SRC = ROOT / "target" / "falcon3"
DEFAULT_OUT = ROOT / "target1" / "FALCON3.V6"

# Falcon3-3B-Base specs (Librarian)
FALCON_HIDDEN = 3072
FALCON_LAYERS = 22
FALCON_HEADS = 12
FALCON_KV_HEADS = 4
FALCON_INTERMEDIATE = 9216
FALCON_VOCAB = 131072
FALCON_MAX_SEQ = 2048
FALCON_ROPE_THETA = 1000042.0
FALCON_TIE = False


def absmean_quantize(mat_f: np.ndarray) -> tuple[np.ndarray, float]:
    """BF16/f32 -> ternary {-1,0,1} via absmean (BitNet)."""
    x = mat_f.astype(np.float32)
    scale = float(np.mean(np.abs(x))) + 1e-6
    q = np.round(x / scale)
    q = np.clip(q, -1, 1).astype(np.int8)
    return q, scale


def unpack_hf_oi(u8: np.ndarray) -> np.ndarray:
    """HF (out/4, in) uint8 -> int8 (out,in). Layout ESTRIDADO (transformers
    integrations/bitnet.py pack/unpack_weights): byte (r,c) guarda as linhas
    r, r+R, r+2R, r+3R (R=out/4) nos pares de bits 0,2,4,6 (LSb-first).
    Mapa codigo->trit: valor = codigo - 1 (00->-1, 01->0, 10->+1);
    codigo 3 (reservado, pack nunca emite) -> 0.
    (A versao consecutiva byte r <-> linhas 4r..4r+3 permutava tudo: pesos
    validos mas semanticamente errados -> salada de palavras.)"""
    out4, inn = u8.shape
    out = out4 * 4
    codes = (u8.reshape(-1, 1) >> (2 * np.arange(4))) & 3  # (out4*inn, 4)
    lut = np.array([-1, 0, 1, 0], dtype=np.int8)
    v = lut[codes].reshape(out4, inn, 4)
    res = np.empty((out, inn), dtype=np.int8)
    for j in range(4):
        res[j * out4:(j + 1) * out4, :] = v[:, :, j]
    return res


def load_state_dict(src_dir: Path) -> tuple[dict, dict]:
    """Carrega safetensors (merge shards) + config.json. Retorna (state, cfg)."""
    cfg_path = src_dir / "config.json"
    if not cfg_path.exists():
        # fallback: procurar em ROOT/target/falcon3
        raise FileNotFoundError(f"config.json nao encontrado em {src_dir}")
    with open(cfg_path, encoding="utf-8") as f:
        cfg = json.load(f)

    # safetensors: single or sharded model-*.safetensors
    shards = sorted(src_dir.glob("model*.safetensors"))
    if not shards:
        shards = sorted(src_dir.glob("*.safetensors"))
    if not shards:
        raise FileNotFoundError(f"nenhum .safetensors em {src_dir}")

    try:
        from safetensors.torch import load_file
        import torch
    except ImportError as e:
        raise SystemExit(f"safetensors/torch nao instalado: {e}. pip install safetensors torch --upgrade")

    state = {}
    for p in shards:
        print(f"[LOAD] {p.name} ({os.path.getsize(p)/1e9:.2f}GB)")
        part = load_file(str(p))
        for k, v in part.items():
            # converte para numpy float32 (torch tensor -> numpy)
            if hasattr(v, "to"):
                # torch tensor
                if v.dtype == torch.bfloat16:
                    arr = v.to(torch.float32).cpu().numpy()
                elif str(v.dtype) == "torch.float8_e4m3fn":
                    arr = v.to(torch.float32).cpu().numpy()
                else:
                    arr = v.cpu().numpy()
            else:
                arr = np.asarray(v)
            state[k] = arr
    return state, cfg


def get_proj_tensor(state: dict, key: str, native: bool, rows: int, cols: int):
    """Recupera tensor ternario (rows,cols) int8 + scale. Suporta BF16 denso e packed u8."""
    arr = state.get(key)
    if arr is None:
        # tenta sem .weight suffix
        arr = state.get(key.replace(".weight", ""))
    if arr is None:
        print(f"  [WARN] {key} ausente -> zeros")
        q, scale = absmean_quantize(np.zeros((rows, cols), dtype=np.float32))
        return q, scale

    # escala nativa do checkpoint (BitNet weight_scale = 1/mean|w|; o forward
    # BitLinear DIVIDE por ela: y = (x_q @ signs)/(x_scale*w_scale)).
    # O .v6/kernel usa a convencao oposta (scale = mean|w|, forward MULTIPLICA;
    # mesma dos arquivos absmean) -> grava o RECIPROCO. Sem isto, matmuls
    # explodem ~256x (scores ~1e4, h ~1e8) e os logits saturam em uniforme.
    # Ausente -> 1.0 (comportamento antigo).
    def _src_scale(default=1.0):
        s = state.get(key.replace(".weight", ".weight_scale"))
        try:
            return 1.0 / float(np.asarray(s).flat[0])
        except Exception:
            return default

    # caso packed HF uint8 (out/4, in). O forward HF faz x@W.T e o .v6 guarda
    # (in,out) com forward x@stored (sem transpose) -> SEMPRE transpor apos
    # o unpack, inclusive no caso quadrado (out,in)==(rows,cols), onde o shape
    # nao acusa e a falta so aparece como cos~0 com mesmo RMS (q/o do 1B).
    if native and arr.dtype == np.uint8:
        # Projecoes retangulares (k/v/gate/up/down) e quadradas (q/o):
        # HF (out/4, in) com (out,in)==(cols,rows) -> unpack + transpor.
        if arr.ndim == 2 and arr.shape[0] * 4 == cols and arr.shape[1] == rows:
            q = unpack_hf_oi(arr)
            return q.astype(np.int8).T.copy(), _src_scale()
        # fallback 1D packed
        if arr.ndim == 1:
            u8 = arr.reshape(rows // 4, cols)
            q = unpack_hf_oi(u8)
            return q.astype(np.int8), _src_scale()

    # se ja ternario int8 {-1,0,1}
    if arr.dtype == np.int8 or (arr.dtype.kind in "iu" and set(np.unique(arr).tolist()) <= {-1, 0, 1}):
        q = arr.astype(np.int8).reshape(rows, cols)
        return q, 1.0
    # se valores JA ternarios em float (trits exatos -1.0/0.0/1.0).
    # Exige integralidade EXATA: denso arredondavel (ex. lm_head BF16 std~0.02)
    # arredonda p/ {-1,0,1} mas NAO e ternario — passava aqui e virava ~100%
    # zeros com scale 1.0 (FALCON3_1B.V6: unembed morto -> logits uniformes).
    # Denso cai no absmean abaixo (correto).
    if arr.dtype.kind == "f" and arr.size == rows * cols and native:
        r = np.round(arr)
        if np.abs(arr - r).max() == 0.0 and set(np.unique(r.astype(np.int8))).issubset({-1, 0, 1}):
            return r.astype(np.int8).reshape(rows, cols), _src_scale()

    # denso BF16/f32 -> ternariza
    mat = arr.astype(np.float32)
    # corrige transposicao se shape invertida (ex: safetensors (in, out) vs (out, in))
    if mat.shape == (cols, rows):
        mat = mat.T
    elif mat.shape != (rows, cols):
        # tenta reshape se flat
        if mat.size == rows * cols:
            mat = mat.reshape(rows, cols)
        else:
            print(f"  [WARN] {key} shape {mat.shape} != ({rows},{cols}) -> reshape flat")
            mat = mat.reshape(rows, cols) if mat.size == rows * cols else np.zeros((rows, cols), dtype=np.float32)
    q, scale = absmean_quantize(mat)
    return q, scale


def convert(source: Path, output: Path, native: bool, repo: str = "") -> None:
    state, cfg = load_state_dict(source)

    hidden = int(cfg.get("hidden_size", FALCON_HIDDEN))
    num_layers = int(cfg.get("num_hidden_layers", FALCON_LAYERS))
    num_heads = int(cfg.get("num_attention_heads", FALCON_HEADS))
    vocab_size = int(cfg.get("vocab_size", FALCON_VOCAB))
    max_seq = int(cfg.get("max_position_embeddings", FALCON_MAX_SEQ))
    intermediate_size = int(cfg.get("intermediate_size", FALCON_INTERMEDIATE))
    num_kv_heads = int(cfg.get("num_key_value_heads", FALCON_KV_HEADS))
    tie = bool(cfg.get("tie_word_embeddings", FALCON_TIE))
    rope_theta = float(cfg.get("rope_theta", FALCON_ROPE_THETA))

    # Falcon3 q_dim == hidden (3072), head_dim = 256
    head_dim = hidden // num_heads
    q_dim = hidden  # spec: q_dim == hidden (3072)
    k_dim = num_kv_heads * head_dim  # 1024
    ffn_group = intermediate_size * q_dim // hidden  # 9216

    print(f"  hidden={hidden} L={num_layers} heads={num_heads} kv={num_kv_heads}")
    print(f"  q_dim={q_dim} k_dim={k_dim} ffn_group={ffn_group} tie={tie} native={native}")
    print(f"  vocab={vocab_size} intermediate={intermediate_size} theta={rope_theta}")

    # Detecta feat: Falcon nao tem inner/ffn_norm (so rms_attn + rms_ffn)
    has_inner = any("attn_sub_norm" in k or "inner" in k for k in state.keys())
    has_ffn = any("ffn_sub_norm" in k for k in state.keys())
    # conforme task: feat bit0/bit1 conforme BitNet -> se quiser forcar, use has_inner/has_ffn da deteccao
    # default Falcon denso = False/False
    has_theta = True
    feat = compute_feat(has_inner, has_ffn, has_theta)

    # unembed Q6K (feat bit3 0x08): se lm_head existe e e DENSO (float),
    # preserva em Q6_K em vez de ternarizar. Ternarizar o unembed derruba a
    # coerencia aberta (loops): prova em tools/probe_dense_unembed.py, onde
    # Q6K empata caractere-a-caractere com o denso. Arquivos antigos (bit
    # limpo) continuam ternarios; o loader le pelo bit (kernel + chat_v6).
    FEAT_UNEMBED_Q6K = 0x08
    _lm_probe = state.get("lm_head.weight", state.get("model.lm_head.weight"))
    if _lm_probe is None:
        for k in state:
            if "lm_head" in k:
                _lm_probe = state[k]
                break
    unembed_q6k = (not tie) and _lm_probe is not None and np.asarray(_lm_probe).dtype.kind == "f"
    if unembed_q6k:
        feat |= FEAT_UNEMBED_Q6K
        print(f"  unembed=Q6_K (lm_head denso preservado, feat|=0x08)")

    # --- v7: provenance + tokenizer embutido (ADR-0115 §2.2) ---
    # So quando output termina em .V7 (.v6 ignora: write_header_v6 intacto).
    prov = None
    tok_bpb1 = b""
    if output.suffix.upper() == ".V7":
        import subprocess as _sp
        try:
            _git = _sp.run(["git", "rev-parse", "HEAD"], capture_output=True,
                           text=True, cwd=str(ROOT)).stdout.strip()
        except Exception:
            _git = ""
        prov = {"repo": repo or "unknown", "revision": "main",
                "conv": "convert_falcon3_to_v6.py native+Q6K-unembed",
                "git": _git or "unknown",
                "scale_conv": 1, "unpack_conv": 1}
        _tok_json = source / "tokenizer.json"
        if _tok_json.is_file():
            try:
                from tools.export_bpe_bin import export as _bpe_export
            except ImportError:
                from export_bpe_bin import export as _bpe_export
            import tempfile as _tf
            with _tf.NamedTemporaryFile(suffix=".bin", delete=False) as _fh:
                _tmp = Path(_fh.name)
            try:
                _rc = _bpe_export(_tok_json, _tmp, sp32=False)
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
    per_layer = (hidden * q_dim + hidden * k_dim * 2 + q_dim * hidden
                 + hidden * ffn_group * 2 + intermediate_size * q_dim)
    num_params += per_layer * num_layers
    if not tie:
        num_params += hidden * vocab_size  # unembed
    # medusa 0

    output.parent.mkdir(parents=True, exist_ok=True)
    _is_v7 = output.suffix.upper() == ".V7"
    with open(output, "wb") as f:
        _hdr = {
            "model_type": MODEL_LLM,
            "num_params": num_params,
            "hidden": hidden,
            "layers": num_layers,
            "heads": num_heads,
            "vocab": vocab_size,
            "max_seq": min(max_seq, 65535),
            "intermediate": intermediate_size,
            "kv_heads": num_kv_heads,
            "q_dim": q_dim,
            "medusa": 0,
            "tie": tie,
            "act_type": ACT_SILU,
            "embed_type": EMBED_Q6K,
            "feat": feat,
        }
        if _is_v7:
            write_header_v7(f, tok_bpb1=tok_bpb1, provenance=prov, **_hdr)
        else:
            write_header_v6(f, **_hdr)
        # embed (vocab, hidden) BF16 -> Q6_K (hidden, vocab) row-major
        emb_key = "model.embed_tokens.weight"
        if emb_key not in state:
            # fallback procura
            for k in state:
                if "embed_tokens" in k:
                    emb_key = k
                    break
        emb = state[emb_key].astype(np.float32)  # (vocab, hidden)
        if emb.shape != (vocab_size, hidden):
            # pode estar transposto
            if emb.shape == (hidden, vocab_size):
                emb = emb.T
        # write_embed espera (hidden, vocab) row-major
        write_embed(f, emb.T, EMBED_Q6K, scale=1.0)
        print(f"  [T] embed {emb.shape} -> Q6_K {((hidden*vocab_size+255)//256)*210} bytes")

        for li in range(num_layers):
            p = f"model.layers.{li}"
            # RMS norms: ordem rms_attn, rms_ffn, rms_inner (se feat&1), rms_ffn_norm (se feat&2)
            for rk in [f"{p}.input_layernorm.weight", f"{p}.post_attention_layernorm.weight"]:
                arr = state.get(rk)
                if arr is None:
                    arr = np.ones(hidden, dtype=np.float32)
                else:
                    arr = arr.astype(np.float32).reshape(-1)
                # garante tamanho hidden
                if arr.size != hidden:
                    arr = np.ones(hidden, dtype=np.float32)
                write_rms(f, arr)
            if has_inner:
                rk = f"{p}.self_attn.attn_sub_norm.weight"
                arr = state.get(rk, np.ones(hidden, dtype=np.float32)).astype(np.float32).reshape(-1)
                write_rms(f, arr)
            if has_ffn:
                rk = f"{p}.mlp.ffn_sub_norm.weight"
                arr = state.get(rk, np.ones(intermediate_size, dtype=np.float32)).astype(np.float32).reshape(-1)
                write_rms(f, arr)

            tensors = [
                (f"{p}.self_attn.q_proj.weight", hidden, q_dim),
                (f"{p}.self_attn.k_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.v_proj.weight", hidden, k_dim),
                (f"{p}.self_attn.o_proj.weight", q_dim, hidden),
                (f"{p}.mlp.gate_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.up_proj.weight", hidden, ffn_group),
                (f"{p}.mlp.down_proj.weight", intermediate_size, q_dim),
            ]
            for key, rows, cols in tensors:
                q, scale = get_proj_tensor(state, key, native, rows, cols)
                write_ternary(f, q.ravel(), scale)

            if li % 5 == 0 or li + 1 == num_layers:
                print(f"  [L] {li}/{num_layers} off={f.tell()//1024}KB")

        # rms_final
        rk = "model.norm.weight"
        arr = state.get(rk, np.ones(hidden, dtype=np.float32)).astype(np.float32).reshape(-1)
        write_rms(f, arr)

        # unembed tied? Falcon3 tie=false -> escreve
        if not tie:
            lk = "lm_head.weight"
            # procura lm_head em state (pode ser model.lm_head.weight)
            lm = state.get("lm_head.weight", state.get("model.lm_head.weight", state.get(lk)))
            if lm is None:
                for k in state:
                    if "lm_head" in k:
                        lm = state[k]
                        break
            if lm is None:
                print("  [WARN] lm_head ausente -> zeros")
                q, scale = absmean_quantize(np.zeros((hidden, vocab_size), dtype=np.float32))
                write_ternary(f, q.ravel(), scale)
            elif unembed_q6k:
                lm = np.asarray(lm, dtype=np.float32)
                if lm.shape == (vocab_size, hidden):
                    lm = lm.T
                elif lm.shape != (hidden, vocab_size):
                    lm = lm.reshape(hidden, vocab_size)
                write_q6k(f, np.ascontiguousarray(lm), hidden, vocab_size)
                f.write(struct.pack("<f", 1.0))
                print(f"  [T] unembed {(hidden, vocab_size)} -> Q6_K {((hidden * vocab_size + 255) // 256) * 210} bytes")
            else:
                lm = lm.astype(np.float32)
                if lm.shape == (vocab_size, hidden):
                    # precisa (hidden, vocab) para write_ternary? loader espera (hidden, vocab)
                    q, scale = get_proj_tensor({lk: lm.T}, lk, native, hidden, vocab_size)
                elif lm.shape == (hidden, vocab_size):
                    q, scale = get_proj_tensor({lk: lm}, lk, native, hidden, vocab_size)
                else:
                    q, scale = absmean_quantize(lm.reshape(hidden, vocab_size))
                write_ternary(f, q.ravel(), scale)
        # theta (feat bit2)
        if has_theta:
            f.write(struct.pack("<f", rope_theta))

    sz = os.path.getsize(output)
    print(f"\n[OK] {output}: {sz:,} bytes ({sz/1024/1024:.1f} MB)")
    _ver = "v7" if output.suffix.upper() == ".V7" else "v6"
    print(f"  {_ver}: act=SILU embed=Q6_K feat=0x{feat:02x} tie={tie} theta={rope_theta}")
    # Espelha para models/ (SESSION_280) — SOMENTE no fluxo default
    # (output FALCON3.V6). Saídas .V7/nomeadas nunca espelham: evita
    # clobberar models/FALCON3.V6 com outro SKU.
    if output.name != "FALCON3.V6":
        print(f"  [MIRROR] skip (output nomeado {output.name})")
    else:
        try:
            mirror = ROOT / "models" / "FALCON3.V6"
            mirror.parent.mkdir(parents=True, exist_ok=True)
            if mirror.resolve() != output.resolve():
                import shutil
                shutil.copy2(output, mirror)
                print(f"[MIRROR] {mirror}: {sz:,} bytes (espelho de {output.name})")
            # Alias .BIN para compat mkfat32 find_falcon3()
            for alias in [ROOT / "target1" / "FALCON3.BIN", ROOT / "models" / "FALCON3.BIN"]:
                if not alias.exists():
                    import shutil
                    shutil.copy2(output, alias)
                    print(f"[MIRROR] {alias}: alias .BIN")
        except Exception as e:
            print(f"[MIRROR] WARN espelho falhou: {e}")


def main():
    ap = argparse.ArgumentParser(description="Falcon3 3B -> .bitnet v6 (dense ou nativo 1.58bit)")
    ap.add_argument("--source", type=Path, default=DEFAULT_SRC, help="dir com config.json + *.safetensors")
    ap.add_argument("--output", type=Path, default=DEFAULT_OUT, help="arquivo .bitnet v6 saida (FALCON3.V6)")
    ap.add_argument("--native", action="store_true", help="entrada ja ternaria (1.58bit): so transpor/unpack, sem re-ternarizar")
    ap.add_argument("--hf-repo", type=str, default=None, help="atalho: baixa via download_falcon3.py e converte")
    ap.add_argument("--repo", type=str, default="", help="repo fonte p/ provenance v7 (ex.: tiiuae/Falcon3-1B-Instruct-1.58bit)")
    args = ap.parse_args()
    src = args.source
    if args.hf_repo:
        # baixa primeiro
        import subprocess
        dl = ROOT / "tools" / "download_falcon3.py"
        variant = "1.58bit" if args.native else "base"
        subprocess.run([sys.executable, str(dl), "--variant", variant], check=True)
        src = DEFAULT_SRC
    convert(src, args.output, args.native, repo=args.repo)


if __name__ == "__main__":
    main()

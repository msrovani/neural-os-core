#!/usr/bin/env python3
"""chat_v6.py — Chat no HOST (Windows/IDE) com modelos .v6 do neural-os-core.

Nao toca no Rust, nao precisa de QEMU: parseia o .v6 (mesmo layout de
tools/bitnet_writer.py + load_model_v6 em cortex.rs), roda o forward em
torch-CPU e abre um loop de chat para testar se o modelo convertido
realmente gera inferencia (ex.: "Qual e o seu nome?").

Uso:
  python tools/chat_v6.py                      # lista os .v6 e pergunta qual
  python tools/chat_v6.py --model target1/FALCON3_1B.V6
  python tools/chat_v6.py --model target1/FALCON3_1B.V6 --ask "Qual e o seu nome?" --max-tokens 40

Tokenizer: precisa do vocab 131k do Falcon3, que NAO esta em disco.
  O script procura `--tokenizer`, depois `target/tokenizer*.json`, senao
  baixa de `tiiuae/<Modelo>-Instruct` no HF e guarda em target/.
  Sem tokenizer: roda em modo numerico (top-5 ids por passo).

E06 Prototype: grammar-constrained draft-verify (--draft-verify)
  python tools/chat_v6.py --model target1/FALCON3_1B.V6 --draft-verify --ask '{"name": "test", "value": 42}'
"""
from __future__ import annotations
import argparse
import json
import os
import re
import struct
import sys
import time
import urllib.request
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple, Set

ROOT = Path(__file__).resolve().parents[1]
try:
    import torch
except ImportError:
    sys.exit("torch nao instalado (pip install torch --index-url https://download.pytorch.org/whl/cpu)")

try:
    import numpy as np
except ImportError:
    sys.exit("numpy nao instalado (pip install numpy)")

MAGIC = 0xBE11BE11
RMS_EPS = 1e-5
ROPE_TABLE_CAP = 2048  # kernel: rope_seq = max_seq.min(2048) (cortex.rs)

HF_REPO = [
    ("10B", "tiiuae/Falcon3-10B-Instruct"),
    ("7B", "tiiuae/Falcon3-7B-Instruct"),
    ("3B", "tiiuae/Falcon3-3B-Instruct"),
    ("1B", "tiiuae/Falcon3-1B-Instruct"),
    ("BASE", "tiiuae/Falcon3-3B-Instruct"),
]

# ── .v6 ────────────────────────────────────────────────────────────────

import os as _os

# LUT codigo->trit. "kernel" = [0,+1,-1,0] (bitnet_writer/pack_ternary, historico
# do repo); "hf" = codigo-1 (transformers/integrations/bitnet.py unpack_weights:
# 00->-1, 01->0, 10->+1). CHATV6_LUT=hf alterna p/ teste A/B.
_LUT_NAME = _os.environ.get("CHATV6_LUT", "kernel")
_LUT = torch.tensor([0, 1, -1, 0], dtype=torch.int8) if _LUT_NAME != "hf" \
    else torch.tensor([-1, 0, 1, 0], dtype=torch.int8)
print(f"[lut] usando LUT '{_LUT_NAME}': {_LUT.tolist()}")


def unpack_ternary(raw: bytes, n: int) -> "torch.Tensor":
    """bytes 4-trits/byte -> int8 tensor flat[n] (01=+1, 10=-1, 00=0). Vetorizado."""
    nbytes = (n + 3) // 4
    b = torch.frombuffer(bytearray(raw[:nbytes]), dtype=torch.uint8).to(torch.long)
    codes = torch.empty(nbytes * 4, dtype=torch.long)
    for i in range(4):
        codes[i::4] = (b >> (2 * i)) & 3
    return _LUT[codes[:n]]


def decode_q6k_fast(data: bytes, rows: int, cols: int) -> np.ndarray:
    """Q6_K -> (rows, cols) f32. Vetorizado; mesma matematica de
    decode_q6k (bitnet_writer.py) / dequantize_q6_k_block (gguf.rs):
      ql[128]+qh[64]+scales[16]+d(f16) por bloco de 256; por half/lane/is:
      q = (low | high<<4) - 32; value = d * scale * q."""
    total = rows * cols
    nb = (total + 255) // 256
    a = np.frombuffer(bytearray(data[:nb * 210]), dtype=np.uint8).reshape(nb, 210)
    ql = a[:, :128].reshape(nb, 2, 64)
    qh = a[:, 128:192].reshape(nb, 2, 32)
    sc = a[:, 192:208].reshape(nb, 2, 8).astype(np.int8)
    d = a[:, 208:210].copy().view("<f2").astype(np.float32).reshape(nb, 1)
    out = np.empty((nb, 256), dtype=np.float32)
    is_idx = np.array([0] * 16 + [1] * 16)
    for h in (0, 1):
        l0 = (ql[:, h, :32] & 0xF).astype(np.int16)
        l1 = (ql[:, h, 32:] & 0xF).astype(np.int16)
        h0 = (ql[:, h, :32] >> 4).astype(np.int16)
        h1 = (ql[:, h, 32:] >> 4).astype(np.int16)
        q1 = (l0 | ((((qh[:, h] >> 0) & 3).astype(np.int16)) << 4)) - 32
        q2 = (l1 | ((((qh[:, h] >> 2) & 3).astype(np.int16)) << 4)) - 32
        q3 = (h0 | ((((qh[:, h] >> 4) & 3).astype(np.int16)) << 4)) - 32
        q4 = (h1 | ((((qh[:, h] >> 6) & 3).astype(np.int16)) << 4)) - 32
        s = sc[:, h].astype(np.float32)
        s0, s2 = s[:, is_idx], s[:, 2 + is_idx]
        s4, s6 = s[:, 4 + is_idx], s[:, 6 + is_idx]
        base = h * 128
        out[:, base:base + 32] = d * s0 * q1
        out[:, base + 32:base + 64] = d * s2 * q2
        out[:, base + 64:base + 96] = d * s4 * q3
        out[:, base + 96:base + 128] = d * s6 * q4
    return out.reshape(-1)[:total].reshape(rows, cols)


# ── W2A8 experimental (E34 ADR-0116: MEDIDO 6x MAIS LENTO no host CPU —
# _int_mm fallback + conversoes/step vs MKL fp32; suite com vereditos
# IDENTICOS ao fp (prova de robustez a ruido 1%). Default off. NAO transfere
# p/ kernel (trade la e outro: int evita libm soft-float — medir no QEMU!).
# y = (x @ W_signs) * file_scale  ==  (xq @ W_signs) * (file_scale / xs)
# Ativado via chat_v6.W2A8 = True (default False = caminho fp32 validado).
# int8 matmul e EXATA (acumulador int32 folgado p/ K<=23040: 127*23040 < 2^31);
# a unica aproximacao e o arredondamento das ativacoes (~1%).
W2A8 = False

# ── KV Quantization modes (E09: ternary KV for memory savings) ──────────
# KV_QUANT = "fp16" | "int8" | "ternary"
#   fp16:   half precision (2 bytes/element) — baseline
#   int8:   absmax per-tensor (1 byte/element) — current INT8 approach
#   ternary: absmean per-tensor, 2-bit trits (-1/0/+1, 4 trits/byte) — E09 proposal
KV_QUANT = "fp16"


def quantize_kv_ternary(kv: torch.Tensor) -> tuple[torch.Tensor, float]:
    """Quantize KV tensor to ternary (2-bit trits: -1/0/+1) via absmean.
    Returns (packed_bytes, scale) where packed_bytes has 4 trits/byte.
    Scale = mean(|kv|) for dequantization: kv_dequant = trits * scale.
    """
    scale = kv.abs().mean().item()
    if scale == 0:
        scale = 1.0
    # Normalize and quantize to trits: -1, 0, +1
    # Threshold at 0.5 * scale for ternary decision
    thresh = 0.5 * scale
    trits = torch.where(kv > thresh, 1, torch.where(kv < -thresh, -1, 0)).to(torch.int8)
    # Pack 4 trits per byte: 2 bits each (00=0, 01=+1, 10=-1, 11=0)
    n = trits.numel()
    nbytes = (n + 3) // 4
    packed = torch.zeros(nbytes, dtype=torch.uint8)
    trits_flat = trits.flatten()
    # Proper packing: each byte holds 4 trits (2 bits each)
    for i in range(nbytes):
        byte_val = 0
        for j in range(4):
            idx = i * 4 + j
            if idx < n:
                t = trits_flat[idx].item()
                # Map: 0->00, 1->01, -1->10
                if t == 1:
                    code = 1  # 01
                elif t == -1:
                    code = 2  # 10
                else:
                    code = 0  # 00
                byte_val |= (code << (2 * j))
        packed[i] = byte_val
    return packed, scale


def dequantize_kv_ternary(packed: torch.Tensor, scale: float, shape: tuple) -> torch.Tensor:
    """Dequantize ternary KV back to fp32."""
    n = 1
    for s in shape:
        n *= s
    nbytes = (n + 3) // 4
    trits = torch.empty(n, dtype=torch.int8)
    for i in range(nbytes):
        byte_val = packed[i].item()
        for j in range(4):
            idx = i * 4 + j
            if idx < n:
                code = (byte_val >> (2 * j)) & 3
                # Map: 00->0, 01->+1, 10->-1, 11->0
                if code == 1:
                    trits[idx] = 1
                elif code == 2:
                    trits[idx] = -1
                else:
                    trits[idx] = 0
    return (trits.to(torch.float32) * scale).reshape(shape)


def quantize_kv_int8(kv: torch.Tensor) -> tuple[torch.Tensor, float]:
    """Quantize KV tensor to INT8 via absmax per-tensor.
    Returns (int8_tensor, scale) where scale = max(|kv|) / 127.
    """
    max_val = kv.abs().max().item()
    if max_val == 0:
        scale = 1.0
    else:
        scale = max_val / 127.0
    q = (kv / scale).round().clamp(-128, 127).to(torch.int8)
    return q, scale


def dequantize_kv_int8(q: torch.Tensor, scale: float) -> torch.Tensor:
    """Dequantize INT8 KV back to fp32."""
    return q.to(torch.float32) * scale


def _mm(x, w, s):
    """Matmul de linear: fp32 (default) ou W2A8-int8 (flag)."""
    if not W2A8:
        return (x @ w.float()) * s
    sh = x.shape
    x2 = x.reshape(-1, sh[-1]).float()
    xs = 127.0 / x2.abs().amax(dim=-1, keepdim=True).clamp_min(1e-5)
    xq = (x2 * xs).round().clamp(-128, 127).to(torch.int8)
    ww = w if w.dtype == torch.int8 else w.to(torch.int8)
    acc = torch._int_mm(xq.contiguous(), ww.contiguous())
    return (acc.to(torch.float32) * (s / xs)).reshape(*sh[:-1], -1)


def build_forbidden_ids(bpe, eos):
    """Ids nunca geraveis (espelha+estende is_special_id do kernel): pieces
    <...> e >>...<<, com UNUSED, vazias e specials classicos — SEMPRE exceto
    eos (parada deliberada). O kernel filtra <...> + para no gibberish_stop;
    aqui tambem >>UNUSED (que la hoje so para via stop) — recomendacao: levar
    a mesma mascara p/ o kernel (deterministico, barato, seguro)."""
    out = set()
    for i, s in bpe.id2.items():
        if i == eos:
            continue
        if not s:
            out.add(i)
        elif (s.startswith("<") and s.endswith(">")) or \
                (s.startswith(">>") and s.endswith("<<")) or \
                ("UNUSED" in s) or s in ("<s>", "</s>", "<unk>", "<pad>", "</line>"):
            out.add(i)
    return out


class V6:
    def __init__(self, path: Path):
        self.path = path
        buf = path.read_bytes()
        o = 0

        def rd(fmt):
            nonlocal o
            v = struct.unpack_from(fmt, buf, o)
            o += struct.calcsize(fmt)
            return v[0] if len(v) == 1 else v

        magic, ver, self.nparams, mtype = rd("<I"), rd("<H"), rd("<Q"), rd("<B")
        o += 3
        assert magic == MAGIC, f"magic {magic:08X}"
        assert ver in (6, 7), f"version {ver} (v6/v7)"
        assert mtype == 0, f"model_type {mtype} (so LLM)"
        self.ver = ver
        (self.hidden, self.layers, self.heads, self.vocab, self.max_seq,
         self.inter, self.kv_heads, self.q_dim, self.medusa) = (
            rd("<H"), rd("<H"), rd("<H"), rd("<I"), rd("<H"),
            rd("<H"), rd("<H"), rd("<H"), rd("<I"))
        self.tie = rd("<4s") == b"TIED"
        tok_type, tok_len = rd("<B"), rd("<I")
        self.tok_type = tok_type
        self.tok_blob = bytes(buf[o:o + tok_len]) if tok_len else b""
        o += tok_len
        self.act, self.embed_type, self.feat = rd("<B"), rd("<B"), rd("<B")
        self.provenance = None
        if ver == 7:
            # provenance (ADR-0115 §2.2): repo/rev/conv/git + convenções
            rlen = rd("<H")
            repo = bytes(buf[o:o + rlen]).decode("utf-8", "replace")
            o += rlen
            rev = bytes(buf[o:o + 40]).rstrip(b"\x00").decode("ascii", "replace")
            o += 40
            clen = rd("<H")
            conv = bytes(buf[o:o + clen]).decode("utf-8", "replace")
            o += clen
            git = bytes(buf[o:o + 40]).rstrip(b"\x00").decode("ascii", "replace")
            o += 40
            sc, uc, _, _ = struct.unpack_from("<BBBB", buf, o)
            o += 4
            self.provenance = {"repo": repo, "rev": rev, "conv": conv,
                               "git": git, "scale_conv": sc, "unpack_conv": uc}
            print(f"[prov] repo={repo} rev={rev} conv={conv} "
                  f"scale_conv={sc} unpack_conv={uc}")
        assert self.act in (0, 1), f"act {self.act} (so SILU=0/RELU2=1)"
        assert self.embed_type == 1, f"embed {self.embed_type} (so Q6_K)"
        assert self.feat & ~0x0F == 0, f"feat 0x{self.feat:02x} (bits 0-3: inner, ffn, theta, unembed_q6k)"
        assert self.medusa == 0, "medusa>0 nao suportado"

        self.head_dim = self.q_dim // self.heads
        self.k_dim = self.kv_heads * self.head_dim
        self.ffn = self.inter * self.q_dim // self.hidden
        n_emb = self.hidden * self.vocab

        def rms(n):
            nonlocal o
            v = torch.frombuffer(bytearray(buf[o:o + 4 * n]), dtype=torch.float32).clone()
            o += 4 * n
            return v

        def tern(rows, cols):
            nonlocal o
            n = rows * cols
            nb = (n + 3) // 4
            w = unpack_ternary(buf[o:o + nb], n).reshape(rows, cols)
            o += nb
            (scale,) = struct.unpack_from("<f", buf, o)
            o += 4
            return w, float(scale)

        # embed Q6_K (hidden, vocab) row-major
        nb = ((n_emb + 255) // 256) * 210
        self.embed = torch.from_numpy(
            decode_q6k_fast(buf[o:o + nb], self.hidden, self.vocab)).to(torch.float32)
        o += nb
        (self.embed_scale,) = struct.unpack_from("<f", buf, o)
        o += 4

        self.blocks = []
        for _ in range(self.layers):
            rq, rf = rms(self.hidden), rms(self.hidden)
            # SubLN (BitNet: attn_sub_norm hidden + ffn_sub_norm inter).
            # None quando ausente (Falcon3) — o forward ignora.
            ri = rms(self.hidden) if self.feat & 1 else None
            rn = rms(self.inter) if self.feat & 2 else None
            q = tern(self.hidden, self.q_dim)
            k = tern(self.hidden, self.k_dim)
            v = tern(self.hidden, self.k_dim)
            pr = tern(self.q_dim, self.hidden)
            g = tern(self.hidden, self.ffn)
            u = tern(self.hidden, self.ffn)
            d = tern(self.inter, self.q_dim)
            self.blocks.append((rq, rf, ri, rn, q, k, v, pr, g, u, d))

        self.rms_final = rms(self.hidden)
        if self.tie:
            self.unembed, self.unembed_scale = None, 1.0
        elif self.feat & 0x08:
            # unembed Q6_K (hidden, vocab) row-major + f32 scale (== 1.0).
            # Decodifica p/ f32 no load (host tem RAM; kernel usa q6k_matmul_row).
            n_emb = self.hidden * self.vocab
            nb = ((n_emb + 255) // 256) * 210
            self.unembed = torch.from_numpy(
                decode_q6k_fast(buf[o:o + nb], self.hidden, self.vocab)).to(torch.float32)
            o += nb
            (self.unembed_scale,) = struct.unpack_from("<f", buf, o)
            o += 4
        else:
            self.unembed, self.unembed_scale = tern(self.hidden, self.vocab)
        (self.theta,) = struct.unpack_from("<f", buf, o)
        o += 4
        print(f"[v6] {path.name}: hidden={self.hidden} L={self.layers} "
              f"heads={self.heads}/{self.kv_heads} vocab={self.vocab} "
              f"inter={self.inter} theta={self.theta:g} bytes_lidos={o}/{len(buf)}")
        # RoPE pre-computado uma vez (tabela cap 2048 = kernel)
        T = min(self.max_seq, ROPE_TABLE_CAP)
        pos = torch.arange(T, dtype=torch.float32).unsqueeze(1)
        inv = self.theta ** (torch.arange(0, self.head_dim, 2, dtype=torch.float32)
                             * (-2.0 / self.head_dim))
        ang = pos * inv
        self.rope_cos, self.rope_sin = torch.cos(ang), torch.sin(ang)

    # ── forward ────────────────────────────────────────────────────
    @staticmethod
    def rmsnorm(x, w):
        return x / torch.sqrt(x.pow(2).mean(-1, keepdim=True) + RMS_EPS) * w

    def forward_layer(self, h, blk, kvc, start):
        rq, rf, ri, rn, q, k, v, pr, g, u, d = blk
        H, HD, HQ = self.hidden, self.head_dim, self.heads
        KV = self.kv_heads
        x = self.rmsnorm(h, rq)
        qv = _mm(x, q[0], q[1])
        kv = _mm(x, k[0], k[1])
        vv = _mm(x, v[0], v[1])
        S = x.shape[-2]
        qv = qv.view(-1, S, HQ, HD)
        kv = kv.view(-1, S, KV, HD)
        vv = vv.view(-1, S, KV, HD)
        cos, sin = self.rope_cos, self.rope_sin
        if start < ROPE_TABLE_CAP:
            nrot = min(S, ROPE_TABLE_CAP - start)
            c, s = cos[start:start + nrot], sin[start:start + nrot]
            # (S,H,HD): aplica nas posicoes cobertas pela tabela
            if self.act == 1:
                # BitNet/HF: rotate_half por metades + cos/sin cat((f,f))
                # (modeling_bitnet.py apply_rotary_pos_emb; loop-for-loop).
                cf = torch.cat([c, c], dim=-1)
                sf = torch.cat([s, s], dim=-1)

                def rot(t):
                    t1, t2 = t[:nrot], t[nrot:]
                    x1, x2 = t1.chunk(2, dim=-1)
                    rh = torch.cat([-x2, x1], dim=-1)
                    r = t1 * cf[:, None, :] + rh * sf[:, None, :]
                    return torch.cat([r, t2], dim=0) if t2.shape[0] else r
            else:
                def rot(t):
                    t1, t2 = t[:nrot], t[nrot:]
                    a, b = t1[..., 0::2], t1[..., 1::2]
                    r = torch.stack([a * c[:, None, :] - b * s[:, None, :],
                                     a * s[:, None, :] + b * c[:, None, :]], -1).flatten(-2)
                    return torch.cat([r, t2], dim=0) if t2.shape[0] else r
            qv, kv = rot(qv.squeeze(0)).unsqueeze(0), rot(kv.squeeze(0)).unsqueeze(0)
        if kvc is not None:
            kk, vv0 = kvc
            # Dequantize cached KV if needed
            if KV_QUANT == "ternary":
                kk = dequantize_kv_ternary(kk[0], kk[1], kk[2])
                vv0 = dequantize_kv_ternary(vv0[0], vv0[1], vv0[2])
            elif KV_QUANT == "int8":
                kk = dequantize_kv_int8(kk[0], kk[1])
                vv0 = dequantize_kv_int8(vv0[0], vv0[1])
            kv = torch.cat([kk, kv], dim=1)
            vv = torch.cat([vv0, vv], dim=1)
            # Quantize and store back to cache
            if KV_QUANT == "ternary":
                k_packed, k_scale = quantize_kv_ternary(kv)
                v_packed, v_scale = quantize_kv_ternary(vv)
                kvc[0] = (k_packed, k_scale, kv.shape)
                kvc[1] = (v_packed, v_scale, vv.shape)
            elif KV_QUANT == "int8":
                k_q, k_scale = quantize_kv_int8(kv)
                v_q, v_scale = quantize_kv_int8(vv)
                kvc[0] = (k_q, k_scale)
                kvc[1] = (v_q, v_scale)
            else:
                kvc[0], kvc[1] = kv, vv
        rep = HQ // KV
        kv_e = kv.repeat_interleave(rep, dim=2)
        vv_e = vv.repeat_interleave(rep, dim=2)
        qh = qv.transpose(1, 2)    # (1,HQ,S,HD)
        kh = kv_e.transpose(1, 2)  # (1,HQ,T,HD)
        vh = vv_e.transpose(1, 2)
        att = (qh @ kh.transpose(-1, -2)) / (HD ** 0.5)  # (1,HQ,S,T)
        T = kv.shape[1]
        # mascara causal: query i (pos global start+i) ve chaves <= start+i
        qi = torch.arange(start, start + S)[:, None]
        kj = torch.arange(T)[None, :]
        att = att.masked_fill((qi < kj).unsqueeze(0).unsqueeze(0), float("-inf"))
        att = torch.softmax(att, -1)
        o = (att @ vh).transpose(1, 2).reshape(-1, S, HQ * HD)
        if ri is not None:
            o = self.rmsnorm(o, ri)  # SubLN: antes do o_proj (HF L214-216)
        h = h + _mm(o, pr[0], pr[1])
        y = self.rmsnorm(h, rf)
        gate = _mm(y, g[0], g[1])
        up = _mm(y, u[0], u[1])
        if self.act == 1:
            fu = torch.relu(gate) ** 2 * up  # ReLU^2 (ReLUSquaredActivation)
        else:
            fu = torch.nn.functional.silu(gate) * up
        if rn is not None:
            fu = self.rmsnorm(fu, rn)  # SubLN: antes do down (HF MLP L76)
        h = h + _mm(fu, d[0], d[1])
        return h

    @torch.no_grad()
    def prefill_collect(self, ids):
        """Prefill devolvendo [h_embed, h_l0, ..., h_lN] (p/ paridade vs referencia)."""
        if KV_QUANT == "ternary":
            kvc = [[(torch.empty(0, dtype=torch.uint8), 1.0, (1, 0, self.kv_heads, self.head_dim)),
                    (torch.empty(0, dtype=torch.uint8), 1.0, (1, 0, self.kv_heads, self.head_dim))]
                   for _ in range(self.layers)]
        elif KV_QUANT == "int8":
            kvc = [[(torch.empty(1, 0, self.kv_heads, self.head_dim, dtype=torch.int8), 1.0),
                    (torch.empty(1, 0, self.kv_heads, self.head_dim, dtype=torch.int8), 1.0)]
                   for _ in range(self.layers)]
        else:
            kvc = [[torch.empty(1, 0, self.kv_heads, self.head_dim),
                    torch.empty(1, 0, self.kv_heads, self.head_dim)]
                   for _ in range(self.layers)]
        h = self.embed[:, list(ids)].T.unsqueeze(0) * self.embed_scale
        states = [h.clone()]
        for li, blk in enumerate(self.blocks):
            h = self.forward_layer(h, blk, kvc[li], 0)
            states.append(h.clone())
        return states

    @torch.no_grad()
    def generate(self, ids, max_tokens, eos, stream=None, _unembed_override=None,
                 temp=0.0, top_p=1.0, seed=None,
                 repetition_penalty=1.0, frequency_penalty=0.0, penalty_last_n=64,
                 forbidden_ids=None):
        if seed is not None:
            torch.manual_seed(seed)
        ids = list(ids)
        if KV_QUANT == "ternary":
            kvc = [[(torch.empty(0, dtype=torch.uint8), 1.0, (1, 0, self.kv_heads, self.head_dim)),
                    (torch.empty(0, dtype=torch.uint8), 1.0, (1, 0, self.kv_heads, self.head_dim))]
                   for _ in range(self.layers)]
        elif KV_QUANT == "int8":
            kvc = [[(torch.empty(1, 0, self.kv_heads, self.head_dim, dtype=torch.int8), 1.0),
                    (torch.empty(1, 0, self.kv_heads, self.head_dim, dtype=torch.int8), 1.0)]
                   for _ in range(self.layers)]
        else:
            kvc = [[torch.empty(1, 0, self.kv_heads, self.head_dim),
                    torch.empty(1, 0, self.kv_heads, self.head_dim)]
                   for _ in range(self.layers)]
        h = self.embed[:, ids].T.unsqueeze(0) * self.embed_scale
        for li, blk in enumerate(self.blocks):
            h = self.forward_layer(h, blk, kvc[li], 0)
        out = []
        nxt = None
        import time
        t0 = time.time()
        for _ in range(max_tokens):
            last = h[:, -1:, :]
            if _unembed_override is not None:
                U, s = _unembed_override
            else:
                U = self.unembed.float() if self.unembed is not None else self.embed
                s = self.unembed_scale
            logits = (self.rmsnorm(last, self.rms_final) @ U) * s
            # Penalidades anti-loop (deterministicas; 1.0/0.0 = off, igual antes).
            # repetition: estilo llama (divide/multiplica se ja apareceu);
            # frequency: -count*penalty (OpenAI). Janela = contexto recente.
            if repetition_penalty != 1.0 or frequency_penalty != 0.0:
                from collections import Counter as _Counter
                _tail = (ids + out)[-penalty_last_n:] if penalty_last_n > 0 else (ids + out)
                _seen = _Counter(t for t in _tail if 0 <= t < logits.shape[-1])
                if _seen:
                    _adj = logits[0, 0]
                    for _t, _c in _seen.items():
                        _v = float(_adj[_t])
                        if repetition_penalty != 1.0 and _v != 0.0:
                            _v = _v / repetition_penalty if _v > 0 else _v * repetition_penalty
                        if frequency_penalty != 0.0:
                            _v -= _c * frequency_penalty
                        _adj[_t] = _v
            if forbidden_ids:
                logits[0, 0][list(forbidden_ids)] = float("-inf")
            if temp > 0:
                probs = torch.softmax(logits[0, 0] / temp, dim=-1)
                if top_p < 1.0:
                    order = torch.argsort(probs, descending=True)
                    ps = probs[order]
                    keep = torch.cumsum(ps, dim=0) <= top_p
                    keep[0] = True
                    mask = torch.zeros_like(probs, dtype=torch.bool)
                    mask[order[keep]] = True
                    probs = torch.where(mask, probs, torch.zeros_like(probs))
                    probs = probs / probs.sum()
                nxt = int(torch.multinomial(probs, 1).item())
            else:
                nxt = int(logits.argmax(-1).item())
            out.append(nxt)
            if stream:
                stream(nxt)
            if nxt == eos:
                break
            h = self.embed[:, [nxt]].T.unsqueeze(0) * self.embed_scale
            for li, blk in enumerate(self.blocks):
                h = self.forward_layer(h, blk, kvc[li], len(ids) + len(out) - 1)
        dt = time.time() - t0
        return out, (len(out) / dt if dt > 0 else 0.0)

    # ── E06: grammar-constrained draft-verify ──────────────────────────
    @torch.no_grad()
    def generate_draft_verify(
        self,
        ids: List[int],
        max_tokens: int,
        eos: int,
        schema: Dict[str, Any],
        bpe: Optional["BPE"],
        acceptance_threshold: float = 0.5,
        max_draft_tokens: int = 8,
        stream=None,
        temp: float = 0.0,
        top_p: float = 1.0,
        seed: Optional[int] = None,
        repetition_penalty: float = 1.0,
        frequency_penalty: float = 0.0,
        penalty_last_n: int = 64,
        forbidden_ids: Optional[Set[int]] = None,
    ) -> Tuple[List[int], float, Dict[str, Any]]:
        """
        Grammar-constrained draft-verify generation (E06 prototype).

        Draft phase: generate tokens constrained by JSON schema using a simple
        state machine that only allows valid JSON tokens at each position.

        Verify phase: run full model forward on draft tokens, accept/reject
        based on probability threshold (acceptance_threshold).

        Returns: (accepted_tokens, tok/s, stats_dict)
        """
        if seed is not None:
            torch.manual_seed(seed)

        # Build JSON syntax-constrained token mask for each position
        schema_machine = JSONSyntaxStateMachine(bpe, self.vocab)

        ids = list(ids)
        kvc = [[torch.empty(1, 0, self.kv_heads, self.head_dim),
                torch.empty(1, 0, self.kv_heads, self.head_dim)]
               for _ in range(self.layers)]
        h = self.embed[:, ids].T.unsqueeze(0) * self.embed_scale
        for li, blk in enumerate(self.blocks):
            h = self.forward_layer(h, blk, kvc[li], 0)

        accepted = []
        total_drafted = 0
        total_accepted = 0
        total_rejected = 0
        verify_calls = 0

        t0 = time.time()

        for step in range(max_tokens):
            # Get allowed tokens from schema state machine
            allowed_tokens = schema_machine.get_allowed_tokens(accepted)
            if not allowed_tokens:
                # Schema complete or no valid continuation
                break

            # Draft phase: generate constrained tokens greedily
            draft_tokens = []
            draft_h = h
            draft_kvc = [[kk.clone(), vv.clone()] for kk, vv in kvc]

            for _ in range(max_draft_tokens):
                last = draft_h[:, -1:, :]
                U = self.unembed.float() if self.unembed is not None else self.embed
                s = self.unembed_scale
                logits = (self.rmsnorm(last, self.rms_final) @ U) * s

                # Apply penalties
                if repetition_penalty != 1.0 or frequency_penalty != 0.0:
                    from collections import Counter as _Counter
                    _tail = (ids + accepted + draft_tokens)[-penalty_last_n:] if penalty_last_n > 0 else (ids + accepted + draft_tokens)
                    _seen = _Counter(t for t in _tail if 0 <= t < logits.shape[-1])
                    if _seen:
                        _adj = logits[0, 0]
                        for _t, _c in _seen.items():
                            _v = float(_adj[_t])
                            if repetition_penalty != 1.0 and _v != 0.0:
                                _v = _v / repetition_penalty if _v > 0 else _v * repetition_penalty
                            if frequency_penalty != 0.0:
                                _v -= _c * frequency_penalty
                            _adj[_t] = _v

                if forbidden_ids:
                    logits[0, 0][list(forbidden_ids)] = float("-inf")

                # Mask to only schema-allowed tokens
                mask = torch.full_like(logits[0, 0], float("-inf"))
                mask[list(allowed_tokens)] = 0
                logits = logits + mask.unsqueeze(0).unsqueeze(0)

                if temp > 0:
                    probs = torch.softmax(logits[0, 0] / temp, dim=-1)
                    if top_p < 1.0:
                        order = torch.argsort(probs, descending=True)
                        ps = probs[order]
                        keep = torch.cumsum(ps, dim=0) <= top_p
                        keep[0] = True
                        mask2 = torch.zeros_like(probs, dtype=torch.bool)
                        mask2[order[keep]] = True
                        probs = torch.where(mask2, probs, torch.zeros_like(probs))
                        probs = probs / probs.sum()
                    nxt = int(torch.multinomial(probs, 1).item())
                else:
                    nxt = int(logits.argmax(-1).item())

                draft_tokens.append(nxt)
                total_drafted += 1

                if nxt == eos:
                    break

                # Update draft state
                draft_h = self.embed[:, [nxt]].T.unsqueeze(0) * self.embed_scale
                for li, blk in enumerate(self.blocks):
                    draft_h = self.forward_layer(draft_h, blk, draft_kvc[li], len(ids) + len(accepted) + len(draft_tokens) - 1)

                # Update schema state
                schema_machine.advance(nxt)
                allowed_tokens = schema_machine.get_allowed_tokens(accepted + draft_tokens)
                if not allowed_tokens or nxt == eos:
                    break

            if not draft_tokens:
                break

            # Verify phase: run full forward on draft tokens, check probabilities
            verify_h = h
            verify_kvc = [[kk.clone(), vv.clone()] for kk, vv in kvc]
            verify_accepted = []

            for draft_tok in draft_tokens:
                verify_calls += 1
                last = verify_h[:, -1:, :]
                U = self.unembed.float() if self.unembed is not None else self.embed
                s = self.unembed_scale
                logits = (self.rmsnorm(last, self.rms_final) @ U) * s

                # Apply same penalties
                if repetition_penalty != 1.0 or frequency_penalty != 0.0:
                    from collections import Counter as _Counter
                    _tail = (ids + accepted + verify_accepted)[-penalty_last_n:] if penalty_last_n > 0 else (ids + accepted + verify_accepted)
                    _seen = _Counter(t for t in _tail if 0 <= t < logits.shape[-1])
                    if _seen:
                        _adj = logits[0, 0]
                        for _t, _c in _seen.items():
                            _v = float(_adj[_t])
                            if repetition_penalty != 1.0 and _v != 0.0:
                                _v = _v / repetition_penalty if _v > 0 else _v * repetition_penalty
                            if frequency_penalty != 0.0:
                                _v -= _c * frequency_penalty
                            _adj[_t] = _v

                if forbidden_ids:
                    logits[0, 0][list(forbidden_ids)] = float("-inf")

                probs = torch.softmax(logits[0, 0], dim=-1)
                tok_prob = probs[draft_tok].item()

                if tok_prob >= acceptance_threshold:
                    # Accept
                    verify_accepted.append(draft_tok)
                    total_accepted += 1
                    # Update verify state
                    verify_h = self.embed[:, [draft_tok]].T.unsqueeze(0) * self.embed_scale
                    for li, blk in enumerate(self.blocks):
                        verify_h = self.forward_layer(verify_h, blk, verify_kvc[li], len(ids) + len(accepted) + len(verify_accepted) - 1)
                else:
                    # Reject: stop at this token, don't include it
                    total_rejected += 1
                    break

            # Commit accepted tokens
            accepted.extend(verify_accepted)
            if stream:
                for t in verify_accepted:
                    stream(t)

            # Update main state with accepted tokens
            for tok in verify_accepted:
                h = self.embed[:, [tok]].T.unsqueeze(0) * self.embed_scale
                for li, blk in enumerate(self.blocks):
                    h = self.forward_layer(h, blk, kvc[li], len(ids) + len(accepted) - 1)

            # Check if schema is complete or EOS
            if schema_machine.is_complete() or (verify_accepted and verify_accepted[-1] == eos):
                break

            # If we rejected early, we need to sample a new token from the model
            # (fallback to unconstrained for the rejected position)
            if total_rejected > 0 and len(verify_accepted) < len(draft_tokens):
                # Sample one token unconstrained to continue
                last = h[:, -1:, :]
                U = self.unembed.float() if self.unembed is not None else self.embed
                s = self.unembed_scale
                logits = (self.rmsnorm(last, self.rms_final) @ U) * s
                if forbidden_ids:
                    logits[0, 0][list(forbidden_ids)] = float("-inf")
                if temp > 0:
                    probs = torch.softmax(logits[0, 0] / temp, dim=-1)
                    nxt = int(torch.multinomial(probs, 1).item())
                else:
                    nxt = int(logits.argmax(-1).item())
                accepted.append(nxt)
                if stream:
                    stream(nxt)
                h = self.embed[:, [nxt]].T.unsqueeze(0) * self.embed_scale
                for li, blk in enumerate(self.blocks):
                    h = self.forward_layer(h, blk, kvc[li], len(ids) + len(accepted) - 1)
                schema_machine.advance(nxt)
                if nxt == eos:
                    break

        dt = time.time() - t0
        tok_s = len(accepted) / dt if dt > 0 else 0.0

        stats = {
            "total_drafted": total_drafted,
            "total_accepted": total_accepted,
            "total_rejected": total_rejected,
            "acceptance_rate": total_accepted / total_drafted if total_drafted > 0 else 0.0,
            "verify_calls": verify_calls,
            "steps": step + 1,
        }
        return accepted, tok_s, stats


# ── E06: JSON Syntax State Machine ───────────────────────────────────

class JSONSyntaxStateMachine:
    """State machine for JSON syntax-constrained token generation.

    Constrains tokens to valid JSON syntax (structural chars, strings, numbers,
    literals) without enforcing specific schema values. Schema validation
    happens post-generation.
    """

    def __init__(self, bpe: Optional["BPE"], vocab_size: int):
        self.bpe = bpe
        self.vocab_size = vocab_size
        self.state = "start"  # start, in_object, in_array, in_string, in_number, after_value, done
        self.depth = 0  # nesting depth
        self.in_string = False
        self.completed = False

        # Precompute token sets for JSON syntax tokens
        self._init_token_sets()

    def _init_token_sets(self):
        """Precompute token IDs for JSON syntax tokens."""
        self.tokens = {
            "structural": set(),      # { } [ ] : ,
            "string_start": set(),    # "
            "string_content": set(),  # string content chars
            "number": set(),          # digits, -, .
            "literal": set(),         # true, false, null
            "whitespace": set(),      # space, newline, tab
        }

        if self.bpe is None:
            return

        # Structural tokens
        structural_chars = {
            "{": "structural", "}": "structural",
            "[": "structural", "]": "structural",
            ":": "structural", ",": "structural",
        }
        for ch, cat in structural_chars.items():
            if ch in self.bpe.vocab:
                self.tokens[cat].add(self.bpe.vocab[ch])

        # String delimiter
        if '"' in self.bpe.vocab:
            self.tokens["string_start"].add(self.bpe.vocab['"'])

        # Number tokens
        for d in "0123456789":
            if d in self.bpe.vocab:
                self.tokens["number"].add(self.bpe.vocab[d])
        if "-" in self.bpe.vocab:
            self.tokens["number"].add(self.bpe.vocab["-"])
        if "." in self.bpe.vocab:
            self.tokens["number"].add(self.bpe.vocab["."])

        # Literals
        for lit in ("true", "false", "null"):
            if lit in self.bpe.vocab:
                self.tokens["literal"].add(self.bpe.vocab[lit])

        # Whitespace
        for ws in (" ", "\n", "\t", "\r"):
            if ws in self.bpe.vocab:
                self.tokens["whitespace"].add(self.bpe.vocab[ws])

        # Also add multi-char tokens that the tokenizer might produce
        for tok, tid in self.bpe.vocab.items():
            if tok in ("true", "false", "null"):
                self.tokens["literal"].add(tid)
            elif tok.startswith('"') and tok.endswith('"') and len(tok) > 2:
                # Quoted string token - can be used as complete string value
                self.tokens["string_content"].add(tid)
            elif all(c in '0123456789.-' for c in tok) and any(c in '0123456789' for c in tok):
                # Number-like token
                self.tokens["number"].add(tid)
            elif tok in ("{", "}", "[", "]", ":", ","):
                self.tokens["structural"].add(tid)

    def get_allowed_tokens(self, generated: List[int]) -> Set[int]:
        """Get set of token IDs allowed at current position based on JSON syntax."""
        if self.completed:
            return set()

        allowed = set()

        # Always allow whitespace
        allowed.update(self.tokens["whitespace"])

        if self.state == "start":
            # Must start with { or [
            allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("{"), self.bpe.vocab.get("[")})

        elif self.state == "in_object":
            if self.depth == 0:
                # At root level, only } allowed to close
                allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("}")})
            elif self.in_string:
                # Inside a string value
                allowed.update(self.tokens["string_content"])
                allowed.update(self.tokens["string_start"])  # Can close string
            else:
                # Expecting key (string) or } or ,
                allowed.update(self.tokens["string_start"])
                allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("}"), self.bpe.vocab.get(",")})

        elif self.state == "in_array":
            if self.in_string:
                allowed.update(self.tokens["string_content"])
                allowed.update(self.tokens["string_start"])
            else:
                # Array values: string, number, literal, object, array, or ]
                allowed.update(self.tokens["string_start"])
                allowed.update(self.tokens["number"])
                allowed.update(self.tokens["literal"])
                allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("["), self.bpe.vocab.get("{")})
                allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("]"), self.bpe.vocab.get(",")})

        elif self.state == "after_colon":
            # After key:value colon, expecting value
            if self.in_string:
                allowed.update(self.tokens["string_content"])
                allowed.update(self.tokens["string_start"])
            else:
                allowed.update(self.tokens["string_start"])
                allowed.update(self.tokens["number"])
                allowed.update(self.tokens["literal"])
                allowed.update(self.tokens["structural"] & {self.bpe.vocab.get("["), self.bpe.vocab.get("{")})

        elif self.state == "after_value":
            # After a value, expect , or } or ]
            allowed.update(self.tokens["structural"] & {self.bpe.vocab.get(","), self.bpe.vocab.get("}"), self.bpe.vocab.get("]")})

        # Filter to vocab range
        return {t for t in allowed if 0 <= t < self.vocab_size}

    def advance(self, token_id: int):
        """Advance state machine with a generated token."""
        if self.completed or self.bpe is None:
            return

        tok_str = self.bpe.id2.get(token_id, "")

        # Skip whitespace
        if tok_str in (" ", "\n", "\t", "\r"):
            return

        if self.state == "start":
            if tok_str == "{":
                self.state = "in_object"
                self.depth = 1
            elif tok_str == "[":
                self.state = "in_array"
                self.depth = 1

        elif self.state == "in_object":
            if self.in_string:
                if tok_str == '"':
                    self.in_string = False
                    self.state = "after_colon"  # After key string, expect colon
            else:
                if tok_str == '"':
                    self.in_string = True
                elif tok_str == ":":
                    self.state = "after_colon"
                elif tok_str == "}":
                    self.depth -= 1
                    if self.depth == 0:
                        self.completed = True
                        self.state = "done"
                    else:
                        self.state = "after_value"
                elif tok_str == ",":
                    # Next key
                    pass
                elif tok_str == "{":
                    self.depth += 1
                elif tok_str == "[":
                    self.state = "in_array"
                    self.depth += 1

        elif self.state == "in_array":
            if self.in_string:
                if tok_str == '"':
                    self.in_string = False
                    self.state = "after_value"
            else:
                if tok_str == '"':
                    self.in_string = True
                elif tok_str == "]":
                    self.depth -= 1
                    if self.depth == 0:
                        self.completed = True
                        self.state = "done"
                    else:
                        self.state = "after_value"
                elif tok_str == ",":
                    pass
                elif tok_str == "{":
                    self.state = "in_object"
                    self.depth += 1
                elif tok_str == "[":
                    self.depth += 1

        elif self.state == "after_colon":
            if tok_str == '"':
                self.in_string = True
                self.state = "in_object"  # Back to object, but in string value
            elif tok_str in ("{", "["):
                if tok_str == "{":
                    self.state = "in_object"
                else:
                    self.state = "in_array"
                self.depth += 1
            elif tok_str in ("true", "false", "null") or (tok_str and (tok_str[0].isdigit() or tok_str[0] == "-")):
                # Literal or number - single token value
                self.state = "after_value"
            # Multi-token numbers/strings handled by staying in after_colon until structural

        elif self.state == "after_value":
            if tok_str == ",":
                if self.depth > 0:
                    # Check if we're in object or array by looking at context
                    # Simplified: assume object if we just had a key:value
                    pass
            elif tok_str in ("}", "]"):
                self.depth -= 1
                if self.depth == 0:
                    self.completed = True
                    self.state = "done"
                else:
                    self.state = "after_value"

    def is_complete(self) -> bool:
        return self.completed


# ── tokenizer (tokenizer.json HF) ─────────────────────────────────────

def bytes_to_unicode():
    bs = list(range(0x21, 0x7F)) + list(range(0xA1, 0xAD)) + list(range(0xAE, 0x100))
    cs = bs[:]
    n = 0
    for b in range(256):
        if b not in bs:
            bs.append(b)
            cs.append(256 + n)
            n += 1
    return {b: chr(c) for b, c in zip(bs, cs)}


class BPE:
    def __init__(self, tok_json: Path):
        tj = json.loads(tok_json.read_text(encoding="utf-8"))
        self.vocab = {s: int(i) for s, i in tj["model"]["vocab"].items()}
        for a in tj.get("added_tokens", []):
            self.vocab.setdefault(a["content"], int(a["id"]))
        self.id2 = {i: s for s, i in self.vocab.items()}
        self.bos = self._find(["<|startoftext|>", "<|begin_of_text|>"], 10)
        self.eos = self._find(["<|endoftext|>", "<|end_of_text|>"], 11)
        self.b2u = bytes_to_unicode()
        self.u2b = {c: b for b, c in self.b2u.items()}
        self.ranks = {}
        for i, m in enumerate(tj["model"].get("merges", [])):
            a, b = (m.split(" ", 1) if isinstance(m, str) else (m[0], m[1]))
            self.ranks[(a, b)] = i

    @classmethod
    def from_bpb1(cls, data: bytes):
        """Constroi BPE de bytes BPB1 (vocab embutido no .v7 / BPE.BIN).
        Espelha __init__: mesmos campos (vocab/id2/bos/eos/b2u/u2b/ranks),
        sem filtrar specials (decode filtra na hora, como no JSON)."""
        assert data[:4] == b"BPB1", "BPB1 magic"
        assert int.from_bytes(data[4:6], "little") == 1, "BPB1 ver"
        bos, eos, eot, vocab_n = struct.unpack_from("<IIII", data, 6)
        o = 22
        offs = struct.unpack_from(f"<{vocab_n + 1}I", data, o)
        o += 4 * (vocab_n + 1)
        heap = data[o:o + offs[-1]]
        o += offs[-1]
        self = cls.__new__(cls)
        self.bos, self.eos = bos, eos
        self.id2 = {}
        for i in range(vocab_n):
            a, b = offs[i], offs[i + 1]
            try:
                s = heap[a:b].decode("utf-8") if 0 <= a <= b <= len(heap) else ""
            except UnicodeDecodeError:
                s = ""
            self.id2[i] = s
        self.vocab = {s: i for i, s in self.id2.items() if s}
        self.b2u = bytes_to_unicode()
        self.u2b = {c: b for b, c in self.b2u.items()}
        self.ranks = {}
        if data[o:o + 4] == b"MRG1":
            mn = struct.unpack_from("<I", data, o + 4)[0]
            o += 8
            for i in range(mn):
                la = struct.unpack_from("<H", data, o)[0]
                o += 2
                a = data[o:o + la].decode("utf-8")
                o += la
                lb = struct.unpack_from("<H", data, o)[0]
                o += 2
                b = data[o:o + lb].decode("utf-8")
                o += lb
                self.ranks[(a, b)] = i
        return self

    def _find(self, names, default):
        for n in names:
            if n in self.vocab:
                return self.vocab[n]
        return default

    def encode(self, text):
        out = []
        # aproximacao ASCII da regex GPT-2 (suficiente p/ teste; \p{L} via re.UNICODE)
        pat = re.compile(
            r"'s|'t|'re|'ve|'m|'ll|'d| ?[^\W\d_]+| ?\d+| ?[^\s\w]+|\s+(?!\S)|\s+")
        for m in pat.finditer(text):
            piece = "".join(self.b2u[b] for b in m.group(0).encode("utf-8"))
            out.extend(self._bpe(piece))
        return out

    def _bpe(self, token):
        if token in self.vocab:
            return [self.vocab[token]]
        word = list(token)
        pairs = {(word[i], word[i + 1]) for i in range(len(word) - 1)}
        while pairs:
            bigram = min(pairs, key=lambda p: self.ranks.get(p, float("inf")))
            if bigram not in self.ranks:
                break
            a, b = bigram
            new, i = [], 0
            while i < len(word):
                if i < len(word) - 1 and (word[i], word[i + 1]) == bigram:
                    new.append(a + b)
                    i += 2
                else:
                    new.append(word[i])
                    i += 1
            word = new
            pairs = {(word[i], word[i + 1]) for i in range(len(word) - 1)} if len(word) > 1 else set()
        return [self.vocab.get(w, 0) for w in word]

    def decode(self, ids):
        parts = []
        buf = bytearray()

        def flush():
            if buf:
                parts.append(bytes(buf).decode("utf-8", errors="replace"))
                buf.clear()

        for i in ids:
            s = self.id2.get(i, "")
            if not s:
                continue
            if (s.startswith("<|") and s.endswith("|>")) or s in (
                    "<s>", "</s>", "<unk>", "<pad>"):
                continue
            for ch in s:
                if ch == "Ġ":
                    flush()
                    parts.append(" ")
                elif ch in self.u2b:
                    buf.append(self.u2b[ch])
                else:
                    flush()
                    parts.append(ch)
        flush()
        return "".join(parts)


# ── modelos / CLI ────────────────────────────────────────────────────

def find_models():
    found = []
    for d in (ROOT / "target1", ROOT / "models"):
        if d.is_dir():
            found += sorted(d.glob("*.V6")) + sorted(d.glob("*.v6"))
    seen, uniq = set(), []
    for p in found:
        if p.resolve() not in seen:
            seen.add(p.resolve())
            uniq.append(p)
    return uniq


def ensure_tokenizer(model_path: Path) -> Path | None:
    for cand in (ROOT / "target" / "tokenizer.json",
                 ROOT / "target" / f"tokenizer_{model_path.stem.lower()}.json"):
        if cand.is_file():
            return cand
    repo = next((r for tag, r in HF_REPO if tag in model_path.stem.upper()), None)
    if repo is None:
        # fallback: qualquer tokenizer Falcon3 ja em cache (mesma familia/vocab)
        cached = sorted((ROOT / "target").glob("tokenizer_falcon3_*.json"))
        if cached:
            print(f"[tok] sem repo p/ {model_path.name} — usando cache {cached[0].name}")
            return cached[0]
        return None
    dst = ROOT / "target" / f"tokenizer_{model_path.stem.lower()}.json"
    if dst.is_file():
        return dst
    url = f"https://huggingface.co/{repo}/resolve/main/tokenizer.json"
    print(f"[tok] baixando {url} ...")
    try:
        urllib.request.urlretrieve(url, dst)
        print(f"[tok] salvo em {dst}")
        return dst
    except Exception as e:
        print(f"[tok] download falhou ({e}) — modo numerico")
        return None


# ── E06: draft-verify test runner ────────────────────────────────────

DEFAULT_SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "value": {"type": "number"}
    },
    "required": ["name", "value"]
}


def json_repair(text: str) -> str:
    """Simple JSON repair: try to extract valid JSON from text."""
    text = text.strip()
    # Try to find JSON object
    start = text.find('{')
    end = text.rfind('}')
    if start >= 0 and end > start:
        candidate = text[start:end+1]
        try:
            json.loads(candidate)
            return candidate
        except json.JSONDecodeError:
            pass
    # Try to find JSON array
    start = text.find('[')
    end = text.rfind(']')
    if start >= 0 and end > start:
        candidate = text[start:end+1]
        try:
            json.loads(candidate)
            return candidate
        except json.JSONDecodeError:
            pass
    return "{}"


def validate_json_schema(data: Any, schema: Dict[str, Any]) -> bool:
    """Simple schema validation for our test schema."""
    if not isinstance(data, dict):
        return False
    if "name" not in data or "value" not in data:
        return False
    if not isinstance(data["name"], str):
        return False
    if not isinstance(data["value"], (int, float)):
        return False
    return True


def run_draft_verify(m: V6, bpe: BPE, args):
    """Run E06 draft-verify prototype test."""
    print("\n=== E06 Draft-Verify Prototype ===")

    # Load schema
    if args.schema:
        schema = json.loads(Path(args.schema).read_text())
        print(f"[schema] loaded from {args.schema}")
    else:
        schema = DEFAULT_SCHEMA
        print(f"[schema] using default: {json.dumps(schema)}")

    # Test prompt that should produce JSON matching schema
    prompt = 'Retorne apenas JSON valido: {"name": "test", "value": 42}'
    ids = [bpe.bos] + bpe.encode(f"<|user|>\n{prompt}\n<|assistant|>\n")
    ids = [i for i in ids if i < m.vocab] or [0]
    eos = bpe.eos
    fbd = build_forbidden_ids(bpe, eos)

    print(f"[prompt] {prompt}")
    print(f"[config] max_tokens={args.max_tokens}, threshold={args.acceptance_threshold}, max_draft={args.max_draft_tokens}")

    # 1. Draft-verify constrained generation
    print("\n--- Draft-Verify (Constrained) ---")
    pieces = []
    def on_tok(t):
        pieces.append(t)
        sys.stdout.write(bpe.decode([t]))
        sys.stdout.flush()

    t0 = time.time()
    out_dv, tps_dv, stats = m.generate_draft_verify(
        ids, args.max_tokens, eos, schema, bpe,
        acceptance_threshold=args.acceptance_threshold,
        max_draft_tokens=args.max_draft_tokens,
        stream=on_tok,
        temp=args.temp, top_p=args.top_p, seed=args.seed,
        repetition_penalty=args.repetition_penalty,
        frequency_penalty=args.frequency_penalty,
        penalty_last_n=args.penalty_last_n,
        forbidden_ids=fbd,
    )
    dt_dv = time.time() - t0
    text_dv = bpe.decode(out_dv)
    print()
    print(f"[dv] {len(out_dv)} toks in {dt_dv:.2f}s ({tps_dv:.2f} tok/s)")
    print(f"[dv] output: {text_dv!r}")
    print(f"[dv] stats: drafted={stats['total_drafted']}, accepted={stats['total_accepted']}, "
          f"rejected={stats['total_rejected']}, acceptance_rate={stats['acceptance_rate']:.2%}, "
          f"verify_calls={stats['verify_calls']}")

    # Validate output
    try:
        parsed_dv = json.loads(text_dv)
        valid_dv = validate_json_schema(parsed_dv, schema)
        print(f"[dv] JSON valid: {valid_dv}, schema valid: {valid_dv}")
    except json.JSONDecodeError as e:
        valid_dv = False
        print(f"[dv] JSON invalid: {e}")

    # 2. Unconstrained greedy generation (baseline)
    print("\n--- Unconstrained Greedy (Baseline) ---")
    pieces = []
    def on_tok2(t):
        pieces.append(t)
        sys.stdout.write(bpe.decode([t]))
        sys.stdout.flush()

    t0 = time.time()
    out_uc, tps_uc = m.generate(
        ids, args.max_tokens, eos, stream=on_tok2,
        temp=args.temp, top_p=args.top_p, seed=args.seed,
        repetition_penalty=args.repetition_penalty,
        frequency_penalty=args.frequency_penalty,
        penalty_last_n=args.penalty_last_n,
        forbidden_ids=fbd,
    )
    dt_uc = time.time() - t0
    text_uc = bpe.decode(out_uc)
    print()
    print(f"[uc] {len(out_uc)} toks in {dt_uc:.2f}s ({tps_uc:.2f} tok/s)")
    print(f"[uc] output: {text_uc!r}")

    # Validate unconstrained output
    try:
        parsed_uc = json.loads(text_uc)
        valid_uc = validate_json_schema(parsed_uc, schema)
        print(f"[uc] JSON valid: {valid_uc}, schema valid: {valid_uc}")
    except json.JSONDecodeError:
        valid_uc = False
        print(f"[uc] JSON invalid")

    # 3. Unconstrained + JSON repair
    if args.compare_unconstrained:
        print("\n--- Unconstrained + JSON Repair ---")
        repaired = json_repair(text_uc)
        try:
            parsed_rep = json.loads(repaired)
            valid_rep = validate_json_schema(parsed_rep, schema)
            print(f"[repair] repaired: {repaired!r}")
            print(f"[repair] JSON valid: {valid_rep}, schema valid: {valid_rep}")
        except json.JSONDecodeError:
            valid_rep = False
            print(f"[repair] repair failed")

    # Summary
    print("\n=== SUMMARY ===")
    speedup = tps_dv / tps_uc if tps_uc > 0 else 0
    print(f"Draft-Verify:  {tps_dv:.2f} tok/s, JSON_valid={valid_dv}, schema_valid={valid_dv}, acceptance={stats['acceptance_rate']:.2%}")
    print(f"Unconstrained: {tps_uc:.2f} tok/s, JSON_valid={valid_uc}, schema_valid={valid_uc}")
    if args.compare_unconstrained:
        print(f"Repaired:      JSON_valid={valid_rep}, schema_valid={valid_rep}")
    print(f"Speedup (dv/uc): {speedup:.2f}x")
    print(f"Acceptance rate: {stats['acceptance_rate']:.2%} ({stats['total_accepted']}/{stats['total_drafted']})")


def main():
    ap = argparse.ArgumentParser(description="Chat no host com modelo .v6")
    ap.add_argument("--model", type=str, default=None)
    ap.add_argument("--tokenizer", type=str, default=None)
    ap.add_argument("--ask", type=str, default=None, help="pergunta unica (nao-interativo)")
    ap.add_argument("--max-tokens", type=int, default=64)
    ap.add_argument("--temp", type=float, default=0.0, help="0=greedy; ex. 0.7 p/ amostrar")
    ap.add_argument("--top-p", type=float, default=1.0)
    ap.add_argument("--seed", type=int, default=None, help="seed p/ sampling deterministico")
    ap.add_argument("--repetition-penalty", type=float, default=1.0, help="1.0=off; ex. 1.15 anti-loop")
    ap.add_argument("--frequency-penalty", type=float, default=0.0)
    ap.add_argument("--penalty-last-n", type=int, default=64)
    ap.add_argument("--kv-quant", type=str, default="fp16", choices=["fp16", "int8", "ternary"],
                    help="KV cache quantization: fp16 (baseline), int8 (absmax), ternary (absmean 2-bit)")
    # E06: draft-verify prototype
    ap.add_argument("--draft-verify", action="store_true", help="E06: grammar-constrained draft-verify mode")
    ap.add_argument("--schema", type=str, default=None, help="JSON schema file for draft-verify")
    ap.add_argument("--acceptance-threshold", type=float, default=0.5, help="Draft token acceptance probability threshold")
    ap.add_argument("--max-draft-tokens", type=int, default=8, help="Max tokens to draft per step")
    ap.add_argument("--compare-unconstrained", action="store_true", help="Compare with unconstrained greedy + JSON repair")
    args = ap.parse_args()

    global KV_QUANT
    KV_QUANT = args.kv_quant
    print(f"[kv] quantization mode: {KV_QUANT}")

    torch.set_num_threads(max(1, os.cpu_count() or 4))

    models = find_models()
    if args.model:
        mp = Path(args.model)
        mp = mp if mp.is_absolute() else ROOT / mp
    else:
        if not models:
            sys.exit("nenhum .v6 em target1/ ou models/")
        print("Modelos .v6:")
        for i, p in enumerate(models):
            print(f"  [{i}] {p.name} ({p.stat().st_size / 1024 / 1024:.0f} MB)")
        mp = models[int(input("escolha [0]: ") or 0)]
    print(f"[load] {mp} ...")
    m = V6(mp)

    bpe = None
    if getattr(m, "tok_type", 0) == 1 and getattr(m, "tok_blob", b""):
        try:
            bpe = BPE.from_bpb1(m.tok_blob)
            print(f"[tok] embutido no arquivo: vocab={len(bpe.vocab)} "
                  f"bos={bpe.bos} eos={bpe.eos} merges={len(bpe.ranks)}")
        except Exception as e:
            print(f"[tok] BPB1 embutido falhou ({e}) — caindo p/ externo")
            bpe = None
    if bpe is None:
        tp = Path(args.tokenizer) if args.tokenizer else ensure_tokenizer(mp)
        if tp and tp.is_file():
            bpe = BPE(tp)
            print(f"[tok] vocab={len(bpe.vocab)} bos={bpe.bos} eos={bpe.eos}")
        else:
            print("[tok] sem vocab — modo numerico (ids, sem texto)")

    def frame(prompt):
        if bpe:
            ids = [bpe.bos] + bpe.encode(f"<|user|>\n{prompt}\n<|assistant|>\n")
        else:
            ids = [10] + [ord(c) % m.vocab for c in prompt[:64]]
        return [i for i in ids if i < m.vocab] or [0]

    fbd = build_forbidden_ids(bpe, bpe.eos) if bpe else None
    if fbd:
        print(f"[gen] {len(fbd)} ids especiais mascarados (nunca gerados, exceto eos)")

    def run(prompt):
        ids = frame(prompt)
        print(f"[gen] prompt={len(ids)} toks ctx<=2048 max_tokens={args.max_tokens}")
        pieces = []

        def on_tok(t):
            pieces.append(t)
            if bpe:
                sys.stdout.write(bpe.decode([t]))
                sys.stdout.flush()
        out, tps = m.generate(ids, args.max_tokens, bpe.eos if bpe else -1,
                              stream=on_tok if bpe else None,
                              temp=args.temp, top_p=args.top_p, seed=args.seed,
                              repetition_penalty=args.repetition_penalty,
                              frequency_penalty=args.frequency_penalty,
                              penalty_last_n=args.penalty_last_n,
                              forbidden_ids=fbd)
        print()
        print(f"[gen] {len(out)} toks {tps:.2f} tok/s")
        if not bpe:
            from collections import Counter
            print("[gen] top ids:", Counter(out).most_common(5))

    # E06: draft-verify test mode
    if args.draft_verify:
        if bpe is None:
            sys.exit("draft-verify requires tokenizer (--tokenizer or embedded BPB1)")
        run_draft_verify(m, bpe, args)
        return

    if args.ask:
        run(args.ask)
        return
    print("chat (vazio sai).")
    while True:
        try:
            p = input("voce> ").strip()
        except EOFError:
            break
        if not p:
            break
        print("modelo> ", end="", flush=True)
        run(p)


if __name__ == "__main__":
    main()

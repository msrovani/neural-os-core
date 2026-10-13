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
"""
from __future__ import annotations
import argparse
import json
import os
import re
import struct
import sys
import urllib.request
from pathlib import Path

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
            kv = torch.cat([kk, kv], dim=1)
            vv = torch.cat([vv0, vv], dim=1)
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
    args = ap.parse_args()

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

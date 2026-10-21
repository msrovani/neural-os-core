#!/usr/bin/env python3
"""export_no_std.py — bare-metal serializer for the BitFuzzy recurrent MoE student.

WHAT THIS IS
    A host-side writer that turns a trained/initialised BitFuzzyRecurrent
    student (tools/bitfuzzy_moe_recurrent.py) into:
      (a) a self-contained binary container (".bfz") that extends the repo's
          canonical .bitnet v6 family with a NEW model_type=3, and
      (b) a C header describing the container (config + tensor table +
          fixed-point routing tables) for a future bare-metal loader.

WHAT THIS IS NOT (honesty)
    * The CURRENT Rust kernel CANNOT load this container.  There is no
      model_type=3 loader in crates/cortex (the loader only knows
      model_type 0/1/2), no kernel path, no runtime proof.  Everything here is
      a HOST-SIDE SERIALIZATION ARTEFACT.  Do not claim kernel readiness.
    * NOTHING here trains anything.  The tool serialises whatever weights the
      student already has and proves that the container reproduces the
      student's logits.  "Trained" is never claimed by this file.
    * The student is NOT purely 2-bit: the token embedding / unembedding
      matrices are real-valued (nn.Embedding / nn.Linear) and the fuzzy
      routing centres+widths are real-valued (stored as i16 fixed point).
      The report prints the exact bytes-per-parameter split.

CONTAINER (model_type=3, "BitFuzzy recurrent MoE")
    Byte 0..51  : the canonical v6 52-byte header (identical field layout to
                  tools/bitnet_writer.py::write_header_v6; model_type=3).
    Byte 52..71 : a 20-byte model_type=3 CONFIG BLOCK (see DEVIATIONS).
    Byte 72..   : the body, strictly sequential, no padding:
        embed -> shared block (rms_attn, rms_ffn, [rms_inner], [rms_ffn_norm],
        q/k/v/o) -> [experts] -> [routing] -> [sugeno masks] -> [step-emb] ->
        act-halting -> rms_final -> unembed -> [theta]
    Tensors are stored as: ternary (2-bit packed trits + f32 scale), f32,
    i16 fixed point or Q6_K, depending on what the student actually uses.
    The feat byte is computed FROM WHAT WAS ACTUALLY WRITTEN (ADR-0085 D5).

DEVIATIONS FROM THE TASK SPEC (all deliberate, all documented in the emitted
manifest / C header / report):
  1. CONFIG BLOCK (20 bytes after the canonical header).  The v6 header has
     no slot for num_experts, top_k, step_embeddings, head_dim,
     use_act_halting, the sugeno-mask count, the LoRA count or rms_eps.
     Without them the file is NOT self-contained and cannot reproduce the
     model's semantics, so they are stored right after the 52-byte header.
  2. PER-ROW SCALE.  The canonical v6 record stores ONE f32 scale per tensor.
     The student's BitLinear/SugenoBitLinear apply a PER-OUTPUT-ROW absmean
     scale, so model_type=3 stores f32[rows] after the packed trits.  The trit
     packing itself is the canonical tools/bitnet_writer.py::pack_ternary.
  3. embed_type=3 (f32, exact).  The embedding/unembedding matrices of this
     student are real-valued, so a ternary or Q6_K encoding would change the
     model semantics and break the parity claim.  embed_type selects the
     encoding of BOTH matrices: 0=ternary, 1=Q6_K, 3=f32(exact).  feat bit
     0x08 ("unembed is Q6_K") is set iff embed_type==1.
  4. ROUTING DEQUANT CONSTANTS.  The i16 centre/width tables are per-column
     affine; the offset/scale vectors (f32[hidden]) travel with them,
     otherwise the integers are meaningless.
  5. SUGENO MASK RECORDS carry a u16 in_features prefix (self-describing).
  6. ACT-HALTING PARAMETERS (halt_center, halt_width, 8 bytes) are stored:
     they change the recurrent output, so dropping them would break parity.
  7. LOW-RANK FACTORS (lora_a/lora_b, f32) have their own section when
     lowrank_rank > 0.

GOLDEN ARTEFACT (--golden)
    python tools/export_no_std.py --golden
    Writes THREE committed files next to this script (the repo pattern: a
    golden blob + a Rust test that reads it with include_bytes!):
      tools/golden_bfz.bin        the model_type=3 container
      tools/golden_bfz.ref.f32    the REFERENCE LOGITS of the SOURCE student
      tools/golden_bfz.meta.json  config + ids + sha256 + regenerate command

    THE TINY GOLDEN STUDENT (exact, documented):
      hidden 32, num_heads 2, num_kv_heads 1, head_dim 16, intermediate 64,
      vocab 64, ONE weight-shared block, recurrent depth K = 3,
      num_experts E = 2, top_k 1, no low-rank (lowrank_rank = 0),
      step_embeddings 3, use_act_halting ON with the DEFAULT threshold
      (act_threshold = 0.9, halt_center = 0.8, halt_width = 0.05),
      max_seq 4096, rms_eps 1e-5, rope_theta 1000042.0, untied lm_head.

    REFERENCE LOGITS FORMAT (tools/golden_bfz.ref.f32):
      raw little-endian f32, row-major, shape [B, T, vocab] = [1, 5, 64]
      (320 floats = 1280 bytes).  NO header bytes, NO magic, NO shape
      prefix: the shape is documented in THIS docstring and in the
      .meta.json, never in the file.  The logits are the SOURCE student's
      (not the rebuilt-from-container student's), so the Rust loader has to
      reproduce them from the CONTAINER alone.

    DETERMINISM (the whole point of the golden):
      1. torch.manual_seed(GOLDEN_SEED=1337) is called immediately BEFORE
         the constructor.  The student module has NO generator argument in
         its init path (nn.init.normal_ / torch.randn), so the GLOBAL CPU
         RNG is the generator and the seed pins it.
      2. The constructor is the only consumer of the global RNG and it
         consumes it in a fixed order, so the weights are a pure function
         of (seed, config, student module source, torch version).
      3. NO RNG at export time: export_bfz only READS the weights.  The
         generator PROVES it by fingerprinting the global RNG state before
         and after the forward and the export.
      4. The input token ids come from an EXPLICIT torch.Generator
         (GOLDEN_ID_SEED=90210), never from the global RNG.
      5. The container is raw bytes (no float formatting) and the .meta.json
         is written with a fixed key order, so both are byte-stable.
      The golden is tied to the torch version recorded in the .meta.json:
      a torch upgrade may change the CPU RNG draw and MUST regenerate it.

CLI
    python tools/export_no_std.py --selftest
    python tools/export_no_std.py --golden
    python tools/export_no_std.py --ckpt student.bitfuzzy --out student.bfz \
        --c-header student.h
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import struct
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import bitfuzzy_moe_recurrent as bf  # noqa: E402
import bitnet_writer as bw  # noqa: E402

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MAGIC = bw.MAGIC            # 0xBE11BE11
VERSION = bw.VERSION        # 6
MODEL_BITFUZZY = 3         # NEW: BitFuzzy recurrent MoE

EMBED_TERNARY = bw.EMBED_TERNARY   # 0
EMBED_Q6K = bw.EMBED_Q6K           # 1
EMBED_F32_EXACT = 3                # extension for model_type=3 (exact f32)

# feat bits (model_type=3)
FEAT_RMS_INNER = 0x01      # rms_inner_attn present
FEAT_RMS_FFN_NORM = 0x02   # rms_ffn_norm present (length = intermediate)
FEAT_THETA = 0x04          # theta present (last 4 bytes of the file)
FEAT_UNEMBED_Q6K = 0x08    # unembed is Q6_K
FEAT_EXPERTS = 0x10        # MoE experts present
FEAT_ROUTING = 0x20        # routing tables present (fixed point)
FEAT_SUGENO = 0x40         # sugeno masks present
FEAT_STEP_EMB = 0x80       # step-embedding table present

HEADER_SIZE = 52           # canonical v6 header
CONFIG_SIZE = 20           # model_type=3 config block
BODY_OFFSET = HEADER_SIZE + CONFIG_SIZE

ROUTING_BITS = 15          # 32767 levels -> fits in int16
Q6K_BLOCK = 210            # bytes per 256-weight Q6_K super-block

NO_SCALE = 0xFFFFFFFF      # scale_offset sentinel in the C descriptor table

KERNEL_STATUS = (
    "LOADER+FWD host-side (crates/cortex/src/bitfuzzy.rs: model_type=3 loader + "
    "recurrent-MoE forward; logit parity VERIFIED against the Python reference). "
    "NOT usable by the boot yet: the hub refuses model_type=3 in every slot, the "
    "forward has never run on bare metal, and the model is untrained."
)

DEVIATIONS = [
    "20-byte model_type=3 config block after the canonical 52-byte header "
    "(num_experts, top_k, step_embeddings, head_dim, flags, mask count, "
    "LoRA count, rms_eps): the v6 header has no slot for them and the file "
    "must be self-contained",
    "ternary records store a PER-ROW f32 scale (f32[rows]) instead of the "
    "canonical single f32: the student's BitLinear/SugenoBitLinear apply a "
    "per-output-row absmean scale (the trit packing is the canonical "
    "bitnet_writer.pack_ternary; the canonical bitnet_writer.write_ternary / "
    "write_embed single-scale convention cannot be used verbatim without "
    "changing the forward)",
    "embed_type=3 (f32 exact) for the embedding/unembedding matrices: they "
    "are real-valued in this student, so ternary/Q6_K would change the model "
    "semantics; embed_type selects the encoding of BOTH matrices",
    "the i16 routing tables carry their per-column affine dequant constants "
    "(f32[hidden] x3) inside the container",
    "sugeno mask records carry a u16 in_features prefix (self-describing)",
    "the ACT-halting parameters (halt_center, halt_width) are stored because "
    "they change the recurrent output",
    "low-rank factors (lora_a/lora_b) get their own f32 section when "
    "lowrank_rank > 0",
]


def _require(cond: bool, msg: str) -> None:
    if not cond:
        raise ValueError(msg)


# ---------------------------------------------------------------------------
# Small binary helpers (all built on the canonical tools/bitnet_writer.py)
# ---------------------------------------------------------------------------


def _pack_trits(trits: np.ndarray) -> bytes:
    """Canonical 2-bit packing (4 trits/byte, LSB first, row-major)."""
    return bw.pack_ternary(np.asarray(trits, dtype=np.int8).ravel())


def _unpack_trits(data: bytes, rows: int, cols: int) -> np.ndarray:
    """Inverse of the canonical packing (reuses the module's unpacker)."""
    packed = torch.from_numpy(np.frombuffer(data, dtype=np.uint8).copy())
    out = bf.ternary_unpack(packed, (int(rows), int(cols)))
    return out.to(torch.int8).numpy()


def _f32_bytes(arr: np.ndarray) -> bytes:
    return np.asarray(arr, dtype="<f4").tobytes()


def _i16_bytes(arr: np.ndarray) -> bytes:
    return np.asarray(arr, dtype="<i2").tobytes()


def _rms_bytes(vec: np.ndarray) -> bytes:
    """Canonical f32 RMS vector writer (no length prefix)."""
    buf = io.BytesIO()
    bw.write_rms(buf, np.asarray(vec, dtype=np.float32))
    return buf.getvalue()


def _q6k_size(n: int) -> int:
    return ((int(n) + 255) // 256) * Q6K_BLOCK


def _tern_len(n: int) -> int:
    return (int(n) + 3) // 4


# ---------------------------------------------------------------------------
# Introspection (walk the student; never hardcode shapes)
# ---------------------------------------------------------------------------


def _introspect(model: Any) -> Dict[str, Any]:
    """Collect every tensor the container needs, by walking the module tree."""
    _require(
        isinstance(model, bf.BitFuzzyRecurrent),
        "model must be a BitFuzzyRecurrent (got %s)" % type(model).__name__,
    )
    cfg = model.cfg
    dense = cfg.dense

    embeds = [m for m in model.modules() if isinstance(m, torch.nn.Embedding)]
    _require(len(embeds) == 1, "expected 1 nn.Embedding, found %d" % len(embeds))
    embed = embeds[0]

    blocks = [m for m in model.modules() if isinstance(m, bf.Falcon3Block)]
    _require(len(blocks) == 1, "expected 1 Falcon3Block, found %d" % len(blocks))
    block = blocks[0]

    moe = block.mlp
    _require(isinstance(moe, bf.BitFuzzyMoE), "block.mlp must be a BitFuzzyMoE")
    attn = block.self_attn

    proj: Dict[str, Any] = {}
    for name in ("q_proj", "k_proj", "v_proj", "o_proj"):
        mod = getattr(attn, name, None)
        _require(
            isinstance(mod, (bf.BitLinear, bf.SugenoBitLinear)),
            "attention projection %s is not a ternary layer" % name,
        )
        proj[name] = mod

    experts = list(moe.experts)
    _require(
        len(experts) == int(moe.num_experts),
        "expert list (%d) != num_experts (%d)" % (len(experts), int(moe.num_experts)),
    )
    for e in experts:
        for sub in ("gate", "up", "down"):
            m = getattr(e, sub, None)
            _require(
                isinstance(m, (bf.BitLinear, bf.SugenoBitLinear)),
                "expert projection %s is not a ternary layer" % sub,
            )

    for name in ("input_layernorm", "post_attention_layernorm"):
        m = getattr(block, name, None)
        _require(isinstance(m, bf.RMSNorm), "block.%s must be an RMSNorm" % name)
    _require(isinstance(model.norm, bf.RMSNorm), "model.norm must be an RMSNorm")

    # Low-rank factors: the forward uses them, so they cannot be dropped.
    lora_modules: List[Tuple[str, Any]] = []
    for name in ("q_proj", "k_proj", "v_proj", "o_proj"):
        if getattr(proj[name], "lora_a", None) is not None:
            lora_modules.append(("attn.%s" % name, proj[name]))
    for ei, e in enumerate(experts):
        for sub in ("gate", "up", "down"):
            m = getattr(e, sub)
            if getattr(m, "lora_a", None) is not None:
                lora_modules.append(("expert%d.%s" % (ei, sub), m))

    # Sugeno masks, in a deterministic traversal order.
    masks: List[Dict[str, Any]] = []
    for ei, e in enumerate(experts):
        for sub in ("gate", "up", "down"):
            m = getattr(e, sub)
            if isinstance(m, bf.SugenoBitLinear):
                masks.append(
                    {
                        "path": "expert%d.%s" % (ei, sub),
                        "module": m,
                        "in_features": int(m.in_features),
                    }
                )

    num_params = 0
    for _n, p in model.named_parameters(remove_duplicate=True):
        num_params += int(p.numel())

    with torch.no_grad():
        spec = {
            "model": model,
            "cfg": cfg,
            "dense": dense,
            "embed": embed,
            "block": block,
            "moe": moe,
            "attn": attn,
            "proj": proj,
            "experts": experts,
            "lora_modules": lora_modules,
            "masks": masks,
            "num_params": num_params,
            "hidden": int(dense.hidden_size),
            "vocab": int(dense.vocab_size),
            "intermediate": int(dense.intermediate_size),
            "num_heads": int(dense.num_heads),
            "num_kv_heads": int(dense.num_kv_heads),
            "head_dim": int(dense.head_dim),
            "q_dim": int(dense.num_heads) * int(dense.head_dim),
            "k_dim": int(dense.num_kv_heads) * int(dense.head_dim),
            "max_seq": int(model.max_seq),
            "k_steps": int(cfg.num_shared_blocks),
            "num_experts": int(moe.num_experts),
            "top_k": int(moe.top_k),
            "step_embeddings": int(model.step_emb.shape[0]),
            "use_act_halting": bool(cfg.use_act_halting),
            "rms_eps": float(dense.rms_eps),
            "rope_theta": float(dense.rope_theta),
            "tied": bool(dense.tie_embeddings),
            "act_type": bw.ACT_SILU,
        }
    return spec


def _ternary_record(mod: Any) -> Tuple[np.ndarray, np.ndarray]:
    """(trits, per-row absmean scale) read from the MODULE (never recomputed)."""
    with torch.no_grad():
        trits = mod.ternary_weight().detach().to(torch.int8).cpu().numpy()
        scale = mod.row_scale().detach().reshape(-1).to(torch.float32).cpu().numpy()
    return trits, scale


def _reconstruct_weight(trits: np.ndarray, scale: np.ndarray) -> np.ndarray:
    """Rebuild a raw weight from (trits, per-row absmean scale).

    The module derives trits = sign(W) with a deadzone and
    scale[m] = absmean(|W[m, :]|).  A raw weight reproducing BOTH is
        W'[m, i] = trits[m, i] * scale[m] / f[m],  f[m] = mean_i |trits[m, i]|
    because absmean(|W'[m, :]|) = scale[m] and |W'[m, i]| = scale[m]/f[m]
    >= scale[m] > 0.05*scale[m] (the deadzone can never flip a trit).
    """
    trits = np.asarray(trits, dtype=np.float32)
    scale = np.asarray(scale, dtype=np.float32).reshape(-1, 1)
    f = np.abs(trits).mean(axis=1, keepdims=True)
    factor = np.where(f > 0.0, scale / np.maximum(f, 1e-30), 0.0)
    return (trits * factor).astype(np.float32)


# ---------------------------------------------------------------------------
# Body builder
# ---------------------------------------------------------------------------


class _Body:
    """Sequential body writer that records the byte range of every record."""

    def __init__(self) -> None:
        self.buf = bytearray()
        self.records: List[Dict[str, Any]] = []

    def _record(
        self,
        name: str,
        payload: bytes,
        *,
        dtype: str,
        rows: int = 0,
        cols: int = 0,
        scale_offset: Optional[int] = None,
        scale_length: int = 0,
        kind: str = "",
        note: str = "",
    ) -> Dict[str, Any]:
        offset = len(self.buf)
        self.buf += payload
        rec = {
            "name": name,
            "offset": offset,
            "length": len(payload),
            "dtype": dtype,
            "rows": int(rows),
            "cols": int(cols),
            "scale_offset": scale_offset,
            "scale_length": int(scale_length),
            "kind": kind,
            "note": note,
        }
        self.records.append(rec)
        return rec

    def ternary(self, name: str, trits: np.ndarray, scale: np.ndarray,
                *, kind: str = "ternary2bit", note: str = "") -> Dict[str, Any]:
        rows, cols = int(trits.shape[0]), int(trits.shape[1])
        payload = _pack_trits(trits)
        data_len = len(payload)
        payload += _f32_bytes(scale)
        off = len(self.buf)
        rec = self._record(
            name, payload, dtype="ternary2bit", rows=rows, cols=cols,
            scale_offset=off + data_len, scale_length=4 * len(scale),
            kind=kind, note=note,
        )
        rec["data_length"] = data_len
        return rec

    def f32(self, name: str, arr: np.ndarray, *, rows: int = 0, cols: int = 0,
            kind: str = "f32", note: str = "") -> Dict[str, Any]:
        arr = np.asarray(arr, dtype=np.float32)
        return self._record(name, _f32_bytes(arr), dtype="f32", rows=rows,
                            cols=cols, kind=kind, note=note)


def _matrix_encoding(spec: Dict[str, Any], embed_encoding: str) -> int:
    """embed_type for the embedding / unembedding matrices."""
    if embed_encoding == "auto":
        # The student's matrices are real-valued nn modules -> exact f32.
        return EMBED_F32_EXACT
    if embed_encoding == "ternary":
        return EMBED_TERNARY
    if embed_encoding == "q6k":
        return EMBED_Q6K
    if embed_encoding == "f32":
        return EMBED_F32_EXACT
    raise ValueError("unknown embed encoding %r" % embed_encoding)


def _write_matrix(body: _Body, name: str, mat: np.ndarray, embed_type: int,
                  *, note: str = "") -> Dict[str, Any]:
    """Write one embedding-shaped matrix ([hidden, vocab], row-major)."""
    mat = np.asarray(mat, dtype=np.float32)
    rows, cols = int(mat.shape[0]), int(mat.shape[1])
    if embed_type == EMBED_TERNARY:
        trits = np.sign(mat).astype(np.int8)
        scale = np.abs(mat).mean(axis=1, keepdims=True).reshape(-1)
        return body.ternary(name, trits, scale, note=note)
    if embed_type == EMBED_Q6K:
        buf = io.BytesIO()
        bw.write_embed(buf, mat, EMBED_Q6K, 1.0)
        payload = buf.getvalue()
        off = len(body.buf)
        rec = body._record(name, payload, dtype="q6k", rows=rows, cols=cols,
                           scale_offset=off + len(payload) - 4, scale_length=4,
                           kind="q6k", note=note)
        rec["data_length"] = len(payload) - 4
        return rec
    # exact f32 (+ a canonical single f32 scale slot, unused for f32)
    payload = _f32_bytes(mat) + struct.pack("<f", 1.0)
    off = len(body.buf)
    rec = body._record(name, payload, dtype="f32", rows=rows, cols=cols,
                       scale_offset=off + len(payload) - 4, scale_length=4,
                       kind="f32", note=note)
    rec["data_length"] = len(payload) - 4
    return rec


def _build_body(spec: Dict[str, Any], embed_encoding: str) -> Tuple[_Body, Dict[str, Any]]:
    """Build the body of the container.  Returns (body, extra_info)."""
    model = spec["model"]
    body = _Body()
    extra: Dict[str, Any] = {"masks": [], "routing": None}

    embed_type = _matrix_encoding(spec, embed_encoding)
    hidden = spec["hidden"]
    vocab = spec["vocab"]
    intermediate = spec["intermediate"]
    q_dim = spec["q_dim"]
    k_dim = spec["k_dim"]

    # -- embedding ---------------------------------------------------------
    # Canonical v6 layout: the matrix is [hidden, vocab] row-major (the
    # student's nn.Embedding weight is [vocab, hidden] -> transpose).
    with torch.no_grad():
        embed_w = spec["embed"].weight.detach().to(torch.float32).cpu().numpy()
    _write_matrix(body, "embed", embed_w.T, embed_type,
                  note="[hidden][vocab] row-major (canonical v6 layout)")

    # -- shared block ------------------------------------------------------
    with torch.no_grad():
        rms_attn = spec["block"].input_layernorm.weight.detach().to(torch.float32).cpu().numpy()
        rms_ffn = spec["block"].post_attention_layernorm.weight.detach().to(torch.float32).cpu().numpy()
    body._record("rms_attn", _rms_bytes(rms_attn), dtype="f32", rows=hidden,
                 kind="f32", note="block.input_layernorm.weight")
    body._record("rms_ffn", _rms_bytes(rms_ffn), dtype="f32", rows=hidden,
                 kind="f32", note="block.post_attention_layernorm.weight")

    # optional norms (absent in this student -> feat bits stay clear)
    inner = getattr(spec["block"].self_attn, "inner_norm", None)
    if inner is not None and hasattr(inner, "weight"):
        with torch.no_grad():
            v = inner.weight.detach().to(torch.float32).cpu().numpy()
        body._record("rms_inner", _rms_bytes(v), dtype="f32", rows=hidden,
                     kind="f32", note="attention inner rms norm")
    ffn_norm = getattr(spec["block"].mlp, "ffn_norm", None)
    if ffn_norm is not None and hasattr(ffn_norm, "weight"):
        with torch.no_grad():
            v = ffn_norm.weight.detach().to(torch.float32).cpu().numpy()
        body._record("rms_ffn_norm", _rms_bytes(v), dtype="f32",
                     rows=intermediate, kind="f32", note="ffn rms norm")

    # q/k/v/o
    for name, key in (("q", "q_proj"), ("k", "k_proj"), ("v", "v_proj"), ("o", "o_proj")):
        trits, scale = _ternary_record(spec["proj"][key])
        body.ternary(name, trits, scale, kind="ternary2bit",
                     note="self_attn.%s (trits + per-row absmean scale)" % key)

    # -- experts -----------------------------------------------------------
    for ei, e in enumerate(spec["experts"]):
        for sub in ("gate", "up", "down"):
            mod = getattr(e, sub)
            trits, scale = _ternary_record(mod)
            body.ternary("expert%d.%s" % (ei, sub), trits, scale,
                         kind="ternary2bit",
                         note="mlp.experts[%d].%s" % (ei, sub))

    # -- routing (fixed point) --------------------------------------------
    moe = spec["moe"]
    fp = moe.quantize_routing_fixed_point(bits=ROUTING_BITS)
    centers_q = fp["centers_q"].to(torch.int32).numpy()
    widths_q = fp["widths_q"].to(torch.int32).numpy()
    centers_off = fp["centers_offset"].to(torch.float32).numpy()
    centers_scale = fp["centers_scale"].to(torch.float32).numpy()
    widths_scale = fp["widths_scale"].to(torch.float32).numpy()
    with torch.no_grad():
        wg_trits = moe.routing_trits().detach().to(torch.int8).cpu().numpy()
        wg_scale = bf._row_absmean(moe.routing).detach().reshape(-1).to(torch.float32).cpu().numpy()

    body._record("routing.centers_q", _i16_bytes(centers_q), dtype="i16",
                 rows=centers_q.shape[0], cols=centers_q.shape[1], kind="i16",
                 note="fixed-point centres (per-column affine)")
    body._record("routing.widths_q", _i16_bytes(widths_q), dtype="i16",
                 rows=widths_q.shape[0], cols=widths_q.shape[1], kind="i16",
                 note="fixed-point widths (per-column affine)")
    body._record("routing.centers_offset", _f32_bytes(centers_off), dtype="f32",
                 rows=hidden, kind="f32", note="dequant offset (per column)")
    body._record("routing.centers_scale", _f32_bytes(centers_scale), dtype="f32",
                 rows=hidden, kind="f32", note="dequant scale (per column)")
    body._record("routing.widths_scale", _f32_bytes(widths_scale), dtype="f32",
                 rows=hidden, kind="f32", note="dequant scale (per column)")
    body.ternary("routing.wg", wg_trits, wg_scale, kind="ternary2bit",
                 note="ternary routing matrix W_g (scale is informational: the "
                      "forward uses the trits only)")
    extra["routing"] = {
        "centers_q": centers_q,
        "widths_q": widths_q,
        "centers_offset": centers_off,
        "centers_scale": centers_scale,
        "widths_scale": widths_scale,
        "wg_packed": _pack_trits(wg_trits),
        "bits": ROUTING_BITS,
    }

    # -- sugeno masks ------------------------------------------------------
    for mi, m in enumerate(spec["masks"]):
        with torch.no_grad():
            trits = m["module"].mask_trits().detach().to(torch.int8).cpu().numpy()
        payload = struct.pack("<H", int(m["in_features"])) + _pack_trits(trits)
        off = len(body.buf)
        rec = body._record("sugeno_mask.%d" % mi, payload, dtype="ternary2bit",
                           rows=1, cols=int(m["in_features"]), kind="ternary2bit",
                           note="mask P of %s (u16 in_features prefix)" % m["path"])
        rec["data_length"] = len(payload) - 2
        extra["masks"].append({"path": m["path"], "in_features": int(m["in_features"]),
                               "trits": trits})

    # -- step embedding ----------------------------------------------------
    with torch.no_grad():
        step_emb = model.step_emb.detach().to(torch.float32).cpu().numpy()
        step_gain = model.step_gain.detach().reshape(-1).to(torch.float32).cpu().numpy()
    step_trits = bf.fuzzy_quantize(
        torch.from_numpy(step_emb), bf.DEFAULT_TEMP
    ).detach().to(torch.int8).numpy()
    rec = body.ternary("step_emb", step_trits, step_gain, kind="ternary2bit",
                       note="ternary step-embedding pool; the f32 vector is the "
                            "per-step gain actually applied by the forward")
    rec["scale_note"] = "step_gain (not an absmean scale)"

    # -- ACT halting -------------------------------------------------------
    with torch.no_grad():
        halt_center = float(model.halt_center.detach().reshape(-1)[0])
        halt_width = float(model.halt_width.detach().reshape(-1)[0])
    body._record("act_halting", struct.pack("<ff", halt_center, halt_width),
                 dtype="f32", rows=2, kind="f32",
                 note="halt_center, halt_width (change the recurrent output)")

    # -- final norm --------------------------------------------------------
    with torch.no_grad():
        rms_final = model.norm.weight.detach().to(torch.float32).cpu().numpy()
    body._record("rms_final", _rms_bytes(rms_final), dtype="f32", rows=hidden,
                 kind="f32", note="model.norm.weight")

    # -- unembed -----------------------------------------------------------
    if spec["tied"]:
        body._record("unembed", b"", dtype="none", rows=0, cols=0, kind="tied",
                     note="tied to the embedding matrix (zero bytes)")
    else:
        with torch.no_grad():
            lm_w = model.lm_head.weight.detach().to(torch.float32).cpu().numpy()
        _write_matrix(body, "unembed", lm_w.T, embed_type,
                      note="[hidden][vocab] row-major (canonical v6 layout)")

    # -- theta (last 4 bytes of the file) ---------------------------------
    body._record("theta", struct.pack("<f", float(spec["rope_theta"])), dtype="f32",
                 rows=1, kind="f32", note="rope theta")

    # -- low-rank factors --------------------------------------------------
    if spec["lora_modules"]:
        payload = bytearray()
        for path, mod in spec["lora_modules"]:
            with torch.no_grad():
                a = mod.lora_a.detach().to(torch.float32).cpu().numpy()
                b = mod.lora_b.detach().to(torch.float32).cpu().numpy()
            payload += struct.pack("<HHHH", int(a.shape[0]), int(a.shape[1]),
                                   int(b.shape[0]), int(b.shape[1]))
            payload += _f32_bytes(a)
            payload += _f32_bytes(b)
        body._record("lora", bytes(payload), dtype="f32", rows=0, cols=0,
                     kind="f32", note="low-rank factors (lora_a, lora_b) in "
                                      "traversal order")
    return body, extra


# ---------------------------------------------------------------------------
# Header
# ---------------------------------------------------------------------------


def _write_header(spec: Dict[str, Any], feat: int, embed_type: int) -> bytes:
    """The canonical v6 52-byte header, model_type=3.

    The field order and the offsets are byte-identical to
    tools/bitnet_writer.py::write_header_v6 (model_type 0/1 branch).
    """
    buf = bytearray()
    buf += struct.pack("<I", MAGIC)                    # 0
    buf += struct.pack("<H", VERSION)                  # 4
    buf += struct.pack("<Q", int(spec["num_params"]))  # 6
    buf += struct.pack("<B", MODEL_BITFUZZY)           # 14
    buf += b"\x00\x00\x00"                             # 15
    buf += struct.pack("<H", spec["hidden"])           # 18
    buf += struct.pack("<H", spec["k_steps"])          # 20  (K recurrent steps)
    buf += struct.pack("<H", spec["num_heads"])        # 22
    buf += struct.pack("<I", spec["vocab"])            # 24
    buf += struct.pack("<H", spec["max_seq"])          # 28
    buf += struct.pack("<H", spec["intermediate"])     # 30
    buf += struct.pack("<H", spec["num_kv_heads"])     # 32
    buf += struct.pack("<H", spec["q_dim"])            # 34
    buf += struct.pack("<I", 0)                        # 36  num_medusa
    buf += (b"TIED" if spec["tied"] else b"\x00" * 4)  # 40
    buf += struct.pack("<B", 0)                        # 44  tok_type
    buf += struct.pack("<I", 0)                        # 45  tok_len
    buf += struct.pack("<B", spec["act_type"])         # 49
    buf += struct.pack("<B", embed_type)               # 50
    buf += struct.pack("<B", feat)                     # 51
    _require(len(buf) == HEADER_SIZE, "header must be 52 bytes")
    return bytes(buf)


def _write_config(spec: Dict[str, Any], num_masks: int, num_lora: int) -> bytes:
    buf = bytearray()
    buf += struct.pack("<H", spec["num_experts"])      # 0
    buf += struct.pack("<H", spec["top_k"])            # 2
    buf += struct.pack("<H", spec["step_embeddings"])  # 4
    buf += struct.pack("<H", spec["head_dim"])         # 6
    buf += struct.pack("<H", 1 if spec["use_act_halting"] else 0)  # 8 flags
    buf += struct.pack("<H", num_masks)                 # 10
    buf += struct.pack("<H", num_lora)                  # 12
    buf += struct.pack("<H", 0)                         # 14
    buf += struct.pack("<f", spec["rms_eps"])           # 16
    _require(len(buf) == CONFIG_SIZE, "config block must be 20 bytes")
    return bytes(buf)


def _canonical_header(spec: Dict[str, Any], feat_low3: int, embed_type: int) -> bytes:
    """Reference header produced by the CANONICAL writer (model_type=0).

    Every shared field is written with the SAME values as our header, so the
    two byte strings must be identical except for model_type (14) and feat
    (51), which are model_type=3 specific.
    """
    buf = io.BytesIO()
    bw.write_header_v6(
        buf,
        model_type=bw.MODEL_LLM,
        num_params=int(spec["num_params"]),
        hidden=int(spec["hidden"]),
        layers=int(spec["k_steps"]),
        heads=int(spec["num_heads"]),
        vocab=int(spec["vocab"]),
        max_seq=int(spec["max_seq"]),
        intermediate=int(spec["intermediate"]),
        kv_heads=int(spec["num_kv_heads"]),
        q_dim=int(spec["q_dim"]),
        medusa=0,
        tie=bool(spec["tied"]),
        tok_type=0,
        tok_data=b"",
        act_type=int(spec["act_type"]),
        embed_type=int(embed_type),
        feat=int(feat_low3),
    )
    return buf.getvalue()


# ---------------------------------------------------------------------------
# Export
# ---------------------------------------------------------------------------


def _load_model(model_or_ckpt: Any) -> Any:
    """Accept a live model, a checkpoint path or a save_bitfuzzy blob."""
    if isinstance(model_or_ckpt, (str, os.PathLike)):
        return bf.load_bitfuzzy(str(model_or_ckpt))
    if isinstance(model_or_ckpt, dict):
        raw_cfg = dict(model_or_ckpt["config"])
        dense = bf.DenseFalconConfig(**raw_cfg["dense"])
        kwargs = {k: v for k, v in raw_cfg.items() if k != "dense"}
        model = bf.BitFuzzyRecurrent(bf.StudentConfig(dense=dense, **kwargs))
        model.load_state_dict(model_or_ckpt["state_dict"])
        return model
    return model_or_ckpt


def export_bfz(model_or_ckpt: Any, out_path: Any, *, embed_encoding: str = "auto",
               **_ignored: Any) -> Dict[str, Any]:
    """Serialise a BitFuzzy student into the .bfz container.

    Returns the manifest (offsets, sizes, tensor list, footprint bytes).
    """
    model = _load_model(model_or_ckpt)
    model.eval()
    spec = _introspect(model)

    body, extra = _build_body(spec, embed_encoding)
    embed_type = _matrix_encoding(spec, embed_encoding)

    # feat is computed FROM WHAT WAS ACTUALLY WRITTEN (ADR-0085 D5).
    names = {r["name"] for r in body.records}
    feat = 0
    if "rms_inner" in names:
        feat |= FEAT_RMS_INNER
    if "rms_ffn_norm" in names:
        feat |= FEAT_RMS_FFN_NORM
    if "theta" in names:
        feat |= FEAT_THETA
    if embed_type == EMBED_Q6K:
        feat |= FEAT_UNEMBED_Q6K
    if any(r["name"].startswith("expert") for r in body.records):
        feat |= FEAT_EXPERTS
    if "routing.centers_q" in names:
        feat |= FEAT_ROUTING
    if any(r["name"].startswith("sugeno_mask") for r in body.records):
        feat |= FEAT_SUGENO
    if "step_emb" in names:
        feat |= FEAT_STEP_EMB

    header = _write_header(spec, feat, embed_type)
    config = _write_config(spec, len(extra["masks"]), len(spec["lora_modules"]))

    sections: List[Dict[str, Any]] = [
        {"name": "header", "offset": 0, "length": HEADER_SIZE, "dtype": "header",
         "rows": 0, "cols": 0, "scale_offset": None, "scale_length": 0,
         "kind": "meta", "note": "canonical v6 header (model_type=3)"},
        {"name": "config", "offset": HEADER_SIZE, "length": CONFIG_SIZE,
         "dtype": "config", "rows": 0, "cols": 0, "scale_offset": None,
         "scale_length": 0, "kind": "meta",
         "note": "model_type=3 config block (see DEVIATIONS)"},
    ]
    for rec in body.records:
        rec = dict(rec)
        rec["offset"] = BODY_OFFSET + rec["offset"]
        if rec.get("scale_offset") is not None:
            rec["scale_offset"] = BODY_OFFSET + rec["scale_offset"]
        sections.append(rec)

    total = BODY_OFFSET + len(body.buf)
    out = Path(out_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("wb") as fh:
        fh.write(header)
        fh.write(config)
        fh.write(bytes(body.buf))

    manifest = _build_manifest(spec, sections, total, feat, embed_type, extra, out)
    return manifest


def _build_manifest(spec: Dict[str, Any], sections: List[Dict[str, Any]], total: int,
                    feat: int, embed_type: int, extra: Dict[str, Any],
                    out: Path) -> Dict[str, Any]:
    by_kind: Dict[str, int] = {}
    for rec in sections:
        by_kind[rec["kind"]] = by_kind.get(rec["kind"], 0) + int(rec["length"])

    # Parameter split: how many parameters live in each kind of field.
    params = {"ternary": 0, "f32": 0, "i16": 0, "q6k": 0, "tied": 0}
    params["ternary"] = 0
    hidden = spec["hidden"]
    q_dim = spec["q_dim"]
    k_dim = spec["k_dim"]
    intermediate = spec["intermediate"]
    vocab = spec["vocab"]
    params["ternary"] += q_dim * hidden + 2 * k_dim * hidden + hidden * q_dim
    params["ternary"] += spec["num_experts"] * (2 * intermediate * hidden + hidden * intermediate)
    params["ternary"] += spec["step_embeddings"] * hidden
    params["ternary"] += spec["num_experts"] * hidden  # routing W_g
    params["i16"] += 2 * spec["num_experts"] * hidden
    params["f32"] += 2 * hidden + hidden  # rms_attn, rms_ffn, rms_final
    params["f32"] += hidden + hidden + hidden  # routing dequant vectors
    params["f32"] += spec["step_embeddings"]  # step gain
    params["f32"] += 2  # halt params
    if spec["tied"]:
        params["f32"] += 0
    else:
        params["f32"] += hidden * vocab
    params["f32"] += hidden * vocab  # embedding
    for _p, mod in spec["lora_modules"]:
        with torch.no_grad():
            params["f32"] += int(mod.lora_a.numel()) + int(mod.lora_b.numel())
    for m in extra["masks"]:
        params["ternary"] += int(m["in_features"])

    num_params = int(spec["num_params"])
    manifest = {
        "path": str(out),
        "format": {
            "magic": "0x%08X" % MAGIC,
            "version": int(VERSION),
            "model_type": int(MODEL_BITFUZZY),
            "header_bytes": HEADER_SIZE,
            "config_bytes": CONFIG_SIZE,
            "body_offset": BODY_OFFSET,
            "total_bytes": int(total),
        },
        "config": {
            "hidden": hidden,
            "k_steps": spec["k_steps"],
            "num_experts": spec["num_experts"],
            "top_k": spec["top_k"],
            "num_heads": spec["num_heads"],
            "num_kv_heads": spec["num_kv_heads"],
            "head_dim": spec["head_dim"],
            "q_dim": q_dim,
            "k_dim": k_dim,
            "vocab": vocab,
            "intermediate": intermediate,
            "max_seq": spec["max_seq"],
            "step_embeddings": spec["step_embeddings"],
            "use_act_halting": bool(spec["use_act_halting"]),
            "rms_eps": spec["rms_eps"],
            "rope_theta": spec["rope_theta"],
            "tied": bool(spec["tied"]),
            "embed_type": int(embed_type),
            "act_type": int(spec["act_type"]),
            "feat": int(feat),
            "num_params": num_params,
        },
        "sections": sections,
        "tensors": [s for s in sections if s["kind"] not in ("meta",)],
        "footprint": {
            "total_bytes": int(total),
            "bytes_by_kind": by_kind,
            "params_by_kind": params,
            "num_params": num_params,
            "bytes_per_param": (float(total) / num_params) if num_params else 0.0,
            "purely_2bit": False,
            "note": (
                "NOT purely 2-bit: the embedding/unembedding matrices are f32 "
                "(real-valued in this student) and the routing centres/widths "
                "are i16 fixed point.  Only the ternary tensors are 2-bit."
            ),
        },
        "routing": {
            "bits": ROUTING_BITS,
            "centers_q": extra["routing"]["centers_q"],
            "widths_q": extra["routing"]["widths_q"],
            "centers_offset": extra["routing"]["centers_offset"],
            "centers_scale": extra["routing"]["centers_scale"],
            "widths_scale": extra["routing"]["widths_scale"],
            "wg_packed": extra["routing"]["wg_packed"],
        },
        "masks": extra["masks"],
        "deviations": list(DEVIATIONS),
        "kernel_support": KERNEL_STATUS,
        "trained": "unknown - this tool serialises existing weights; it never trains",
    }
    return manifest


# ---------------------------------------------------------------------------
# Reader (independent of the writer)
# ---------------------------------------------------------------------------


def read_bfz(path: Any) -> Dict[str, Any]:
    """Read a .bfz container back into tensors + a manifest.

    Independent reader: it re-derives every length from the header + config
    block instead of trusting the writer's records.
    """
    data = Path(path).read_bytes()
    _require(len(data) >= BODY_OFFSET, "file too small to be a .bfz container")
    magic, version, num_params, model_type = struct.unpack_from("<IHQB", data, 0)
    _require(magic == MAGIC, "bad magic 0x%08X" % magic)
    _require(version == VERSION, "bad version %d" % version)
    _require(model_type == MODEL_BITFUZZY,
             "model_type %d is not the BitFuzzy container (3)" % model_type)

    hidden, k_steps, num_heads = struct.unpack_from("<HHH", data, 18)
    (vocab,) = struct.unpack_from("<I", data, 24)
    max_seq, intermediate, kv_heads, q_dim = struct.unpack_from("<HHHH", data, 28)
    (medusa,) = struct.unpack_from("<I", data, 36)
    tie_flag = bytes(data[40:44])
    tok_type = data[44]
    (tok_len,) = struct.unpack_from("<I", data, 45)
    act_type, embed_type, feat = struct.unpack_from("<BBB", data, 49)

    (num_experts, top_k, step_embeddings, head_dim, flags, num_masks, num_lora,
     _pad) = struct.unpack_from("<8H", data, HEADER_SIZE)
    (rms_eps,) = struct.unpack_from("<f", data, HEADER_SIZE + 16)

    k_dim = kv_heads * head_dim
    tensors: Dict[str, np.ndarray] = {}
    scales: Dict[str, np.ndarray] = {}
    sections: List[Dict[str, Any]] = [
        {"name": "header", "offset": 0, "length": HEADER_SIZE, "dtype": "header"},
        {"name": "config", "offset": HEADER_SIZE, "length": CONFIG_SIZE,
         "dtype": "config"},
    ]

    off = BODY_OFFSET

    def take(nbytes: int) -> bytes:
        nonlocal off
        _require(off + nbytes <= len(data), "truncated container at offset %d" % off)
        chunk = data[off:off + nbytes]
        off += nbytes
        return chunk

    def take_matrix(name: str, rows: int, cols: int) -> None:
        nonlocal off
        n = rows * cols
        if embed_type == EMBED_TERNARY:
            payload = take(_tern_len(n))
            scale = take(4 * rows)
            tensors[name] = _unpack_trits(payload, rows, cols)
            scales[name] = np.frombuffer(scale, dtype="<f4").copy()
            sections.append({"name": name, "offset": off - _tern_len(n) - 4 * rows,
                             "length": _tern_len(n) + 4 * rows, "dtype": "ternary2bit"})
        elif embed_type == EMBED_Q6K:
            payload = take(_q6k_size(n))
            take(4)
            tensors[name] = bw.decode_q6k(payload, rows, cols)
            sections.append({"name": name, "offset": off - _q6k_size(n) - 4,
                             "length": _q6k_size(n) + 4, "dtype": "q6k"})
        else:
            payload = take(4 * n)
            take(4)
            tensors[name] = np.frombuffer(payload, dtype="<f4").copy().reshape(rows, cols)
            sections.append({"name": name, "offset": off - 4 * n - 4,
                             "length": 4 * n + 4, "dtype": "f32"})

    def take_ternary(name: str, rows: int, cols: int) -> None:
        nonlocal off
        n = rows * cols
        payload = take(_tern_len(n))
        scale = take(4 * rows)
        tensors[name] = _unpack_trits(payload, rows, cols)
        scales[name] = np.frombuffer(scale, dtype="<f4").copy()
        sections.append({"name": name, "offset": off - _tern_len(n) - 4 * rows,
                         "length": _tern_len(n) + 4 * rows, "dtype": "ternary2bit"})

    def take_f32(name: str, count: int) -> None:
        nonlocal off
        payload = take(4 * count)
        tensors[name] = np.frombuffer(payload, dtype="<f4").copy()
        sections.append({"name": name, "offset": off - 4 * count,
                         "length": 4 * count, "dtype": "f32"})

    take_matrix("embed", hidden, vocab)
    take_f32("rms_attn", hidden)
    take_f32("rms_ffn", hidden)
    if feat & FEAT_RMS_INNER:
        take_f32("rms_inner", hidden)
    if feat & FEAT_RMS_FFN_NORM:
        take_f32("rms_ffn_norm", intermediate)
    take_ternary("q", q_dim, hidden)
    take_ternary("k", k_dim, hidden)
    take_ternary("v", k_dim, hidden)
    take_ternary("o", hidden, q_dim)
    if feat & FEAT_EXPERTS:
        for e in range(num_experts):
            take_ternary("expert%d.gate" % e, intermediate, hidden)
            take_ternary("expert%d.up" % e, intermediate, hidden)
            take_ternary("expert%d.down" % e, hidden, intermediate)
    if feat & FEAT_ROUTING:
        payload = take(2 * num_experts * hidden)
        tensors["routing.centers_q"] = np.frombuffer(payload, dtype="<i2").copy().reshape(
            num_experts, hidden)
        payload = take(2 * num_experts * hidden)
        tensors["routing.widths_q"] = np.frombuffer(payload, dtype="<i2").copy().reshape(
            num_experts, hidden)
        take_f32("routing.centers_offset", hidden)
        take_f32("routing.centers_scale", hidden)
        take_f32("routing.widths_scale", hidden)
        take_ternary("routing.wg", num_experts, hidden)
    if feat & FEAT_SUGENO:
        for mi in range(num_masks):
            (in_features,) = struct.unpack_from("<H", take(2))
            payload = take(_tern_len(in_features))
            tensors["sugeno_mask.%d" % mi] = _unpack_trits(payload, 1, in_features).reshape(-1)
            sections.append({"name": "sugeno_mask.%d" % mi,
                             "offset": off - 2 - _tern_len(in_features),
                             "length": 2 + _tern_len(in_features),
                             "dtype": "ternary2bit"})
    if feat & FEAT_STEP_EMB:
        # the record is [trits][gain]; the gain IS the scale slot
        take_ternary("step_emb", step_embeddings, hidden)
        tensors["step_gain"] = scales.pop("step_emb")
    take_f32("act_halting", 2)
    take_f32("rms_final", hidden)
    if tie_flag == b"TIED":
        sections.append({"name": "unembed", "offset": off, "length": 0,
                         "dtype": "tied"})
    else:
        take_matrix("unembed", hidden, vocab)
    if feat & FEAT_THETA:
        take_f32("theta", 1)
    if num_lora:
        lora: List[Tuple[np.ndarray, np.ndarray]] = []
        lora_start = off
        for _ in range(num_lora):
            a_out, a_in, b_rows, b_cols = struct.unpack_from("<HHHH", take(8))
            a = np.frombuffer(take(4 * a_out * a_in), dtype="<f4").copy().reshape(a_out, a_in)
            b = np.frombuffer(take(4 * b_rows * b_cols), dtype="<f4").copy().reshape(b_rows, b_cols)
            lora.append((a, b))
        tensors["lora"] = lora  # type: ignore[assignment]
        sections.append({"name": "lora", "offset": lora_start,
                         "length": off - lora_start, "dtype": "f32"})

    _require(off == len(data),
             "container has %d trailing bytes (cursor at %d)" % (len(data) - off, off))

    header = {
        "magic": magic, "version": version, "num_params": num_params,
        "model_type": model_type, "hidden": hidden, "k_steps": k_steps,
        "num_heads": num_heads, "vocab": vocab, "max_seq": max_seq,
        "intermediate": intermediate, "kv_heads": kv_heads, "q_dim": q_dim,
        "medusa": medusa, "tied": tie_flag == b"TIED", "tok_type": tok_type,
        "tok_len": tok_len, "act_type": act_type, "embed_type": embed_type,
        "feat": feat,
    }
    config = {
        "num_experts": num_experts, "top_k": top_k,
        "step_embeddings": step_embeddings, "head_dim": head_dim,
        "use_act_halting": bool(flags & 1), "num_masks": num_masks,
        "num_lora": num_lora, "rms_eps": rms_eps,
    }
    manifest = {
        "header": header,
        "config": config,
        "sections": sections,
        "total_bytes": len(data),
    }
    return {"header": header, "config": config, "tensors": tensors,
            "scales": scales, "manifest": manifest}


# ---------------------------------------------------------------------------
# Rebuild
# ---------------------------------------------------------------------------


def rebuild_student_from_bfz(path: Any, cfg: Any = None) -> Any:
    """Rebuild a BitFuzzyRecurrent from a .bfz container.

    ``cfg`` may be a StudentConfig (or a dict); when None the config is
    derived from the container itself (the container is self-contained).
    """
    blob = read_bfz(path)
    h = blob["header"]
    c = blob["config"]
    tensors = blob["tensors"]
    scales = blob["scales"]

    if cfg is None:
        dense = bf.DenseFalconConfig(
            hidden_size=h["hidden"],
            num_layers=1,             # ONE weight-shared block (recurrent)
            num_heads=h["num_heads"],
            num_kv_heads=h["kv_heads"],
            head_dim=c["head_dim"],
            intermediate_size=h["intermediate"],
            vocab_size=h["vocab"],
            rms_eps=c["rms_eps"],
            rope_theta=float(tensors["theta"][0]) if "theta" in tensors else bf.DEFAULT_ROPE_THETA,
            tie_embeddings=h["tied"],
        )
        cfg = bf.StudentConfig(
            dense=dense,
            num_shared_blocks=h["k_steps"],
            num_experts=c["num_experts"],
            top_k=c["top_k"],
            step_embeddings=c["step_embeddings"],
            use_act_halting=c["use_act_halting"],
            lowrank_rank=0,
        )
    elif isinstance(cfg, dict):
        raw = dict(cfg)
        dense = bf.DenseFalconConfig(**raw["dense"])
        kwargs = {k: v for k, v in raw.items() if k != "dense"}
        cfg = bf.StudentConfig(dense=dense, **kwargs)

    # fail closed on a config that disagrees with the container
    _require(int(cfg.num_shared_blocks) == h["k_steps"],
             "cfg.num_shared_blocks != container K")
    _require(int(cfg.num_experts) == c["num_experts"], "cfg.num_experts mismatch")
    _require(int(cfg.top_k) == c["top_k"], "cfg.top_k mismatch")
    _require(int(cfg.step_embeddings) == c["step_embeddings"],
             "cfg.step_embeddings mismatch")

    model = bf.BitFuzzyRecurrent(cfg)
    model.eval()

    with torch.no_grad():
        # embedding / unembedding ([hidden, vocab] -> [vocab, hidden])
        model.embed_tokens.weight.copy_(
            torch.from_numpy(np.ascontiguousarray(tensors["embed"].T)))
        if h["tied"]:
            model.lm_head.weight = model.embed_tokens.weight
        else:
            model.lm_head.weight.copy_(
                torch.from_numpy(np.ascontiguousarray(tensors["unembed"].T)))

        model.block.input_layernorm.weight.copy_(
            torch.from_numpy(tensors["rms_attn"]))
        model.block.post_attention_layernorm.weight.copy_(
            torch.from_numpy(tensors["rms_ffn"]))
        model.norm.weight.copy_(torch.from_numpy(tensors["rms_final"]))

        for key, name in (("q", "q_proj"), ("k", "k_proj"), ("v", "v_proj"),
                          ("o", "o_proj")):
            mod = getattr(model.block.self_attn, name)
            w = _reconstruct_weight(tensors[key], scales[key])
            mod.weight.copy_(torch.from_numpy(w))

        for e in range(c["num_experts"]):
            expert = model.block.mlp.experts[e]
            for sub in ("gate", "up", "down"):
                mod = getattr(expert, sub)
                w = _reconstruct_weight(tensors["expert%d.%s" % (e, sub)],
                                        scales["expert%d.%s" % (e, sub)])
                mod.weight.copy_(torch.from_numpy(w))

    # masks (the reader does not know the order; rebuild it from the model)
    if c["num_masks"]:
        mask_modules = []
        for e in model.block.mlp.experts:
            for sub in ("gate", "up", "down"):
                mod = getattr(e, sub)
                if isinstance(mod, bf.SugenoBitLinear):
                    mask_modules.append(mod)
        _require(len(mask_modules) == c["num_masks"],
                 "mask count mismatch (%d vs %d)" % (len(mask_modules), c["num_masks"]))
        with torch.no_grad():
            for mi, mod in enumerate(mask_modules):
                trits = tensors["sugeno_mask.%d" % mi]
                # any raw mask whose re-quantisation yields these trits is fine
                mod.mask.copy_(torch.from_numpy(np.asarray(trits, dtype=np.float32)))

    # routing: dequantise the fixed-point tables
    if h["feat"] & FEAT_ROUTING:
        with torch.no_grad():
            centers = (tensors["routing.centers_q"].astype(np.float32)
                       * tensors["routing.centers_scale"].reshape(1, -1)
                       + tensors["routing.centers_offset"].reshape(1, -1))
            widths = (tensors["routing.widths_q"].astype(np.float32)
                      * tensors["routing.widths_scale"].reshape(1, -1))
            model.block.mlp.centers.copy_(torch.from_numpy(centers.astype(np.float32)))
            model.block.mlp.widths.copy_(torch.from_numpy(widths.astype(np.float32)))
            wg = _reconstruct_weight(tensors["routing.wg"], scales["routing.wg"])
            model.block.mlp.routing.copy_(torch.from_numpy(wg))

    # step embedding + gain
    if h["feat"] & FEAT_STEP_EMB:
        with torch.no_grad():
            model.step_emb.copy_(torch.from_numpy(
                np.asarray(tensors["step_emb"], dtype=np.float32)))
            model.step_gain.copy_(torch.from_numpy(
                np.asarray(tensors["step_gain"], dtype=np.float32)))

    # ACT halting
    with torch.no_grad():
        model.halt_center.copy_(torch.tensor([float(tensors["act_halting"][0])]))
        model.halt_width.copy_(torch.tensor([float(tensors["act_halting"][1])]))

    # low-rank factors (if any)
    lora = tensors.get("lora")
    if lora:
        mods = []
        for name in ("q_proj", "k_proj", "v_proj", "o_proj"):
            mod = getattr(model.block.self_attn, name)
            if getattr(mod, "lora_a", None) is not None:
                mods.append(mod)
        for e in model.block.mlp.experts:
            for sub in ("gate", "up", "down"):
                mod = getattr(e, sub)
                if getattr(mod, "lora_a", None) is not None:
                    mods.append(mod)
        _require(len(mods) == len(lora), "LoRA tensor count mismatch")
        with torch.no_grad():
            for mod, (a, b) in zip(mods, lora):
                mod.lora_a.copy_(torch.from_numpy(a))
                mod.lora_b.copy_(torch.from_numpy(b))

    return model


# ---------------------------------------------------------------------------
# C header
# ---------------------------------------------------------------------------


def _c_array(name: str, ctype: str, values: Sequence[Any], per_line: int = 12) -> str:
    lines = ["static const %s %s[%d] = {" % (ctype, name, len(values))]
    row: List[str] = []
    for i, v in enumerate(values):
        if ctype == "int16_t":
            row.append("%d" % int(v))
        elif ctype == "uint8_t":
            row.append("0x%02X" % int(v))
        else:
            row.append("%.9gf" % float(v))
        if len(row) == per_line or i == len(values) - 1:
            lines.append("    " + ", ".join(row) + ("," if i != len(values) - 1 else ""))
            row = []
    lines.append("};")
    return "\n".join(lines)


def emit_c_header(path: Any, manifest: Dict[str, Any]) -> str:
    """Write the C header describing the container."""
    cfg = manifest["config"]
    fp = manifest["footprint"]
    sections = manifest["sections"]
    routing = manifest["routing"]

    L: List[str] = []
    L.append("/*")
    L.append(" * bfz.h - descriptor of a BitFuzzy recurrent MoE container (.bfz)")
    L.append(" *")
    L.append(" * GENERATED by tools/export_no_std.py - do not edit by hand.")
    L.append(" *")
    L.append(" * CONTAINER VERSION : %d   (canonical .bitnet v6 family header)"
             % manifest["format"]["version"])
    L.append(" * MODEL_TYPE        : %d  (BitFuzzy recurrent MoE: ONE weight-shared" %
             manifest["format"]["model_type"])
    L.append(" *                              transformer block applied K recurrent steps)")
    L.append(" * MAGIC             : %s" % manifest["format"]["magic"])
    L.append(" * TOTAL BYTES       : %d" % fp["total_bytes"])
    L.append(" *")
    L.append(" * !!! KERNEL STATUS !!!")
    L.append(" * The CURRENT cortex loader does NOT support model_type=%d." %
             manifest["format"]["model_type"])
    L.append(" * There is no Rust loader, no kernel path and no runtime proof for")
    L.append(" * this container.  It is a HOST-SIDE SERIALISATION ARTEFACT only.")
    L.append(" * Do NOT claim that the kernel can load it.")
    L.append(" *")
    L.append(" * The model is NOT purely 2-bit: the embedding/unembedding matrices")
    L.append(" * are f32 (exact) and the routing centres/widths are i16 fixed point.")
    L.append(" * Only the ternary tensors below are 2-bit (4 trits/byte).")
    L.append(" *")
    L.append(" * TERNARY LAYOUT (canonical): 4 trits/byte, LSB first, row-major over")
    L.append(" * the flattened (rows, cols) matrix.  Code 00=0, 01=+1, 10=-1.")
    L.append(" * The f32 scale that follows the trits is PER OUTPUT ROW (absmean).")
    L.append(" *")
    L.append(" * Documented deviations from the canonical v6 record:")
    for d in manifest["deviations"]:
        wrapped = _wrap_comment(d, 74)
        L.append(" *   - %s" % wrapped)
    L.append(" */")
    L.append("")
    L.append("#ifndef BFZ_H")
    L.append("#define BFZ_H")
    L.append("")
    L.append("#include <stdint.h>")
    L.append("")
    L.append("/* ---- config ---- */")
    L.append("#define BFZ_VERSION            %d" % manifest["format"]["version"])
    L.append("#define BFZ_MODEL_TYPE        %d" % manifest["format"]["model_type"])
    L.append("#define BFZ_MAGIC              0x%08Xu" % MAGIC)
    L.append("#define BFZ_HEADER_BYTES      %d" % manifest["format"]["header_bytes"])
    L.append("#define BFZ_CONFIG_BYTES      %d" % manifest["format"]["config_bytes"])
    L.append("#define BFZ_BODY_OFFSET       %d" % manifest["format"]["body_offset"])
    L.append("#define BFZ_FILE_SIZE         %d" % fp["total_bytes"])
    L.append("")
    L.append("#define BFZ_HIDDEN             %d" % cfg["hidden"])
    L.append("#define BFZ_K_STEPS            %d   /* recurrent steps over ONE block */" % cfg["k_steps"])
    L.append("#define BFZ_EXPERTS            %d" % cfg["num_experts"])
    L.append("#define BFZ_TOP_K              %d" % cfg["top_k"])
    L.append("#define BFZ_HEADS              %d" % cfg["num_heads"])
    L.append("#define BFZ_KV_HEADS           %d" % cfg["num_kv_heads"])
    L.append("#define BFZ_HEAD_DIM           %d" % cfg["head_dim"])
    L.append("#define BFZ_Q_DIM              %d" % cfg["q_dim"])
    L.append("#define BFZ_K_DIM              %d" % cfg["k_dim"])
    L.append("#define BFZ_VOCAB              %d" % cfg["vocab"])
    L.append("#define BFZ_INTERMEDIATE       %d" % cfg["intermediate"])
    L.append("#define BFZ_MAX_SEQ            %d" % cfg["max_seq"])
    L.append("#define BFZ_STEP_EMBEDDINGS    %d" % cfg["step_embeddings"])
    L.append("#define BFZ_USE_ACT_HALTING    %d" % (1 if cfg["use_act_halting"] else 0))
    L.append("#define BFZ_TIED               %d" % (1 if cfg["tied"] else 0))
    L.append("#define BFZ_EMBED_TYPE         %d   /* 0=ternary 1=Q6_K 3=f32(exact) */" % cfg["embed_type"])
    L.append("#define BFZ_ACT_TYPE           %d   /* 0=silu */" % cfg["act_type"])
    L.append("#define BFZ_NUM_PARAMS         %duL" % int(cfg["num_params"]))
    L.append("#define BFZ_BYTES_PER_PARAM    %.6ff" % fp["bytes_per_param"])
    L.append("")
    L.append("/* feat bits (model_type=%d)" % manifest["format"]["model_type"])
    L.append(" * 0x01 rms_inner_attn   0x02 rms_ffn_norm   0x04 theta")
    L.append(" * 0x08 unembed is Q6_K  0x10 MoE experts     0x20 routing tables")
    L.append(" * 0x40 sugeno masks     0x80 step-embedding table")
    L.append(" */")
    L.append("#define BFZ_FEAT               0x%02X" % cfg["feat"])
    L.append("")
    L.append("/* ---- tensor / section descriptors ---- */")
    L.append("typedef struct {")
    L.append("    const char *name;")
    L.append("    uint32_t    offset;       /* byte offset in the container           */")
    L.append("    uint32_t    length;       /* byte length of the record              */")
    L.append("    uint32_t    rows;")
    L.append("    uint32_t    cols;")
    L.append("    uint32_t    scale_offset; /* f32 scale (0xFFFFFFFF = none)         */")
    L.append("    uint32_t    scale_length; /* bytes of the scale                    */")
    L.append("    const char *dtype;")
    L.append("} bfz_record;")
    L.append("")
    L.append("#define BFZ_RECORD_COUNT       %d" % len(sections))
    L.append("")
    L.append("static const bfz_record BFZ_RECORDS[BFZ_RECORD_COUNT] = {")
    for rec in sections:
        so = rec.get("scale_offset")
        so_txt = "0xFFFFFFFFu" if so is None else "%du" % int(so)
        L.append('    { "%s", %du, %du, %du, %du, %s, %du, "%s" },'
                 % (rec["name"], int(rec["offset"]), int(rec["length"]),
                    int(rec.get("rows", 0)), int(rec.get("cols", 0)), so_txt,
                    int(rec.get("scale_length", 0)), rec["dtype"]))
    L.append("};")
    L.append("")
    L.append("/* ---- fixed-point routing tables (integers only) ---- */")
    L.append("/* centres/widths are per-column affine: value = q * scale + offset */")
    L.append("#define BFZ_ROUTING_BITS       %d" % routing["bits"])
    L.append("#define BFZ_ROUTING_ELEMS      %d" % int(routing["centers_q"].size))
    L.append("")
    L.append(_c_array("bfz_routing_centers_q", "int16_t", routing["centers_q"].reshape(-1)))
    L.append("")
    L.append(_c_array("bfz_routing_widths_q", "int16_t", routing["widths_q"].reshape(-1)))
    L.append("")
    L.append("/* ternary routing matrix W_g (4 trits/byte, LSB first) */")
    L.append(_c_array("bfz_routing_wg_packed", "uint8_t", list(routing["wg_packed"])))
    L.append("")
    L.append("/* dequantisation constants (NOT part of the fixed-point tables):")
    L.append(" * per-column affine factors needed to interpret the i16 values. */")
    L.append(_c_array("bfz_routing_centers_offset_f32", "float",
                      routing["centers_offset"].reshape(-1)))
    L.append("")
    L.append(_c_array("bfz_routing_centers_scale_f32", "float",
                      routing["centers_scale"].reshape(-1)))
    L.append("")
    L.append(_c_array("bfz_routing_widths_scale_f32", "float",
                      routing["widths_scale"].reshape(-1)))
    L.append("")
    L.append("#endif /* BFZ_H */")

    text = "\n".join(L) + "\n"
    out = Path(path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="ascii")
    return text


def _wrap_comment(text: str, width: int) -> str:
    words = text.split()
    lines: List[str] = []
    cur = ""
    for w in words:
        if cur and len(cur) + 1 + len(w) > width:
            lines.append(cur)
            cur = w
        else:
            cur = (cur + " " + w) if cur else w
    if cur:
        lines.append(cur)
    return ("\n *     ").join(lines)


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def verify_parity(model_or_ckpt: Any, container_path: Any, *,
                  num_samples: int = 2, seq_len: int = 8) -> Dict[str, Any]:
    """Rebuild the student from the container and compare the logits.

    This is the artifact-level proof: the container semantics must equal the
    model semantics.  Returns the measured deviation (never a claim).
    """
    model = _load_model(model_or_ckpt)
    model.eval()
    rebuilt = rebuild_student_from_bfz(container_path)
    rebuilt.eval()
    vocab = int(model.cfg.dense.vocab_size)
    g = torch.Generator().manual_seed(777)
    ids = torch.randint(0, vocab, (num_samples, seq_len), generator=g)
    with torch.no_grad():
        src_logits, src_steps = model(ids)
        new_logits, new_steps = rebuilt(ids)
    delta = float(torch.max(torch.abs(src_logits - new_logits)))
    scale = max(float(torch.max(torch.abs(src_logits))), 1e-12)
    return {
        "max_abs_delta": delta,
        "rel_delta": delta / scale,
        "src_steps": int(src_steps),
        "rebuilt_steps": int(new_steps),
        "tolerance": 1e-3,
        "ok": bool(delta / scale <= 1e-3 and int(src_steps) == int(new_steps)),
    }


def format_report(manifest: Dict[str, Any]) -> str:
    fp = manifest["footprint"]
    cfg = manifest["config"]
    L: List[str] = []
    L.append("=== BitFuzzy container report ===")
    L.append("path            : %s" % manifest["path"])
    L.append("container       : version %d, model_type %d, magic %s"
             % (manifest["format"]["version"], manifest["format"]["model_type"],
                manifest["format"]["magic"]))
    L.append("header path     : LOCAL writer (canonical write_header_v6 emits only "
             "model_type 0/1/2) + byte-parity check against the canonical writer")
    L.append("total bytes     : %d (header %d + config %d + body %d)"
             % (fp["total_bytes"], manifest["format"]["header_bytes"],
                manifest["format"]["config_bytes"],
                fp["total_bytes"] - manifest["format"]["body_offset"]))
    L.append("parameters      : %d" % fp["num_params"])
    L.append("bytes / param   : %.4f" % fp["bytes_per_param"])
    L.append("purely 2-bit    : NO - %s" % fp["note"])
    L.append("kernel support  : %s" % manifest["kernel_support"])
    L.append("")
    L.append("-- config --")
    for k in ("hidden", "k_steps", "num_experts", "top_k", "num_heads",
              "num_kv_heads", "head_dim", "q_dim", "vocab", "intermediate",
              "max_seq", "step_embeddings", "use_act_halting", "tied",
              "embed_type", "feat"):
        L.append("  %-18s %s" % (k, cfg[k]))
    L.append("")
    L.append("-- bytes per section --")
    for rec in manifest["sections"]:
        L.append("  %-26s off=%-8d len=%-9d %s"
                 % (rec["name"], int(rec["offset"]), int(rec["length"]), rec["dtype"]))
    L.append("")
    L.append("-- bytes per kind --")
    for kind, nbytes in sorted(fp["bytes_by_kind"].items()):
        L.append("  %-14s %d bytes" % (kind, nbytes))
    L.append("")
    L.append("-- parameters per kind (field precision) --")
    for kind, n in sorted(fp["params_by_kind"].items()):
        if n:
            L.append("  %-14s %d params" % (kind, n))
    L.append("")
    L.append("-- deviations --")
    for d in manifest["deviations"]:
        L.append("  - %s" % d)
    return "\n".join(L)


# ---------------------------------------------------------------------------
# Self test
# ---------------------------------------------------------------------------


def _tiny_student() -> Any:
    torch.manual_seed(20261019)
    dense = bf.DenseFalconConfig(
        hidden_size=32,
        num_layers=2,
        num_heads=2,
        num_kv_heads=1,
        head_dim=16,
        intermediate_size=64,
        vocab_size=64,
        tie_embeddings=False,
    )
    scfg = bf.StudentConfig(
        dense=dense,
        num_shared_blocks=3,
        num_experts=2,
        top_k=1,
        step_embeddings=3,
        use_act_halting=True,
        lowrank_rank=0,
    )
    model = bf.BitFuzzyRecurrent(scfg)
    model.eval()
    return model


def _rel_err(a: np.ndarray, b: np.ndarray) -> float:
    a = np.asarray(a, dtype=np.float64)
    b = np.asarray(b, dtype=np.float64)
    denom = max(float(np.max(np.abs(a))), 1e-12)
    return float(np.max(np.abs(a - b))) / denom


def selftest() -> int:
    import tempfile

    results: List[Tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        results.append((name, bool(ok), detail))

    print("== BitFuzzy no_std exporter self test ==")
    print("NOTE: no kernel loader exists for model_type=3; this is a")
    print("      host-side serialisation + parity proof only.  No training.")
    print("HEADER PATH: LOCAL writer (the canonical write_header_v6 only emits")
    print("             model_type 0/1/2) + a byte-parity check against the")
    print("             canonical writer for every shared field.")
    print()

    model = _tiny_student()
    g = torch.Generator().manual_seed(4242)
    ids = torch.randint(0, 64, (2, 4), generator=g)

    tmpdir = Path(tempfile.mkdtemp(prefix="bfz_selftest_"))
    out_path = tmpdir / "tiny.bfz"
    header_path = tmpdir / "tiny.h"

    manifest = export_bfz(model, out_path)
    blob = read_bfz(out_path)
    rebuilt = rebuild_student_from_bfz(out_path)
    rebuilt.eval()
    text = emit_c_header(header_path, manifest)

    # --- 1. header parity with the canonical writer ----------------------
    spec = _introspect(model)
    feat_low3 = manifest["config"]["feat"] & 0x07
    canonical = _canonical_header(spec, feat_low3, manifest["config"]["embed_type"])
    ours = out_path.read_bytes()[:HEADER_SIZE]
    diffs = [i for i in range(HEADER_SIZE)
             if i not in (14, 51) and ours[i] != canonical[i]]
    check("header_canonical_parity", not diffs,
          "model_type(14) and feat(51) excluded; %d differing bytes"
          % len(diffs))
    check("header_feat_low_bits", feat_low3 == bw.compute_feat(False, False, True),
          "feat&0x07 == compute_feat(inner=%s, ffn=%s, theta=%s)"
          % (bool(feat_low3 & 1), bool(feat_low3 & 2), bool(feat_low3 & 4)))

    # --- 2. tensor round trip --------------------------------------------
    t = blob["tensors"]
    s = blob["scales"]
    worst_f32 = 0.0
    exact_ok = True
    detail = []
    for key, mod in (("q", spec["proj"]["q_proj"]), ("k", spec["proj"]["k_proj"]),
                     ("v", spec["proj"]["v_proj"]), ("o", spec["proj"]["o_proj"])):
        src_trits, src_scale = _ternary_record(mod)
        if not np.array_equal(t[key], src_trits):
            exact_ok = False
            detail.append("%s trits differ" % key)
        worst_f32 = max(worst_f32, _rel_err(src_scale, s[key]))
    for e in range(spec["num_experts"]):
        expert = spec["experts"][e]
        for sub in ("gate", "up", "down"):
            src_trits, src_scale = _ternary_record(getattr(expert, sub))
            name = "expert%d.%s" % (e, sub)
            if not np.array_equal(t[name], src_trits):
                exact_ok = False
                detail.append("%s trits differ" % name)
            worst_f32 = max(worst_f32, _rel_err(src_scale, s[name]))
    # step embedding trits
    step_src = bf.fuzzy_quantize(model.step_emb.detach(), bf.DEFAULT_TEMP).to(torch.int8).numpy()
    if not np.array_equal(t["step_emb"], step_src):
        exact_ok = False
        detail.append("step_emb trits differ")
    # norms / matrices
    with torch.no_grad():
        rms_attn = model.block.input_layernorm.weight.detach().numpy()
        rms_ffn = model.block.post_attention_layernorm.weight.detach().numpy()
        rms_final = model.norm.weight.detach().numpy()
        embed_w = model.embed_tokens.weight.detach().numpy()
        lm_w = model.lm_head.weight.detach().numpy()
    for name, src in (("rms_attn", rms_attn), ("rms_ffn", rms_ffn),
                      ("rms_final", rms_final)):
        worst_f32 = max(worst_f32, _rel_err(src, t[name]))
    worst_f32 = max(worst_f32, _rel_err(embed_w, t["embed"].T))
    worst_f32 = max(worst_f32, _rel_err(lm_w, t["unembed"].T))
    check("tensor_roundtrip", exact_ok and worst_f32 <= 1e-3,
          "trits exact=%s, worst f32 rel err=%.3e %s"
          % (exact_ok, worst_f32, ("; " + ", ".join(detail)) if detail else ""))

    # --- 3. logits parity -------------------------------------------------
    with torch.no_grad():
        src_logits, src_steps = model(ids)
        new_logits, new_steps = rebuilt(ids)
    diff = float(torch.max(torch.abs(src_logits - new_logits)))
    scale = max(float(torch.max(torch.abs(src_logits))), 1e-12)
    rel = diff / scale
    check("logits_parity", rel <= 1e-3 and int(src_steps) == int(new_steps),
          "max|delta|=%.3e rel=%.3e steps src=%s rebuilt=%s"
          % (diff, rel, int(src_steps), int(new_steps)))

    # --- 4. C header layout + manifest coverage --------------------------
    rows = re.findall(
        r'\{\s*"([^"]+)"\s*,\s*(\d+)u,\s*(\d+)u,\s*(\d+)u,\s*(\d+)u,\s*'
        r'(0x[0-9A-Fa-f]+|\d+)u,\s*(\d+)u,\s*"([^"]+)"\s*\}',
        text,
    )
    parsed = [(n, int(o), int(l), int(ro), int(co), so, int(sl), dt)
              for (n, o, l, ro, co, so, sl, dt) in rows]
    mono = all(parsed[i][1] == parsed[i - 1][1] + parsed[i - 1][2]
               for i in range(1, len(parsed)))
    covers = bool(parsed) and parsed[0][1] == 0 and \
        parsed[-1][1] + parsed[-1][2] == manifest["footprint"]["total_bytes"]
    file_size = out_path.stat().st_size
    sum_sections = sum(int(r["length"]) for r in manifest["sections"])
    check("c_header_layout", mono and covers and len(parsed) == len(manifest["sections"]),
          "%d records, monotonic=%s, covers file=%s" % (len(parsed), mono, covers))
    check("manifest_covers_file", sum_sections == file_size,
          "sum(sections)=%d file=%d" % (sum_sections, file_size))

    # --- 5. no float literals in the fixed-point tables -------------------
    int_arrays_ok = True
    for name in ("bfz_routing_centers_q", "bfz_routing_widths_q"):
        m = re.search(r"static const int16_t %s\[\d+\] = \{(.*?)\};" % name, text, re.S)
        if not m:
            int_arrays_ok = False
            break
        body_txt = m.group(1)
        if re.search(r"\.", body_txt) or re.search(r"[eE][-+]?\d", body_txt):
            int_arrays_ok = False
    check("routing_tables_are_int", int_arrays_ok,
          "no floating point literals in the fixed-point tables")

    print()
    print("-- footprint --")
    fp = manifest["footprint"]
    for kind, nbytes in sorted(fp["bytes_by_kind"].items()):
        print("  %-14s %8d bytes" % (kind, nbytes))
    print("  %-14s %8d bytes" % ("TOTAL", fp["total_bytes"]))
    print("  parameters      %8d" % fp["num_params"])
    print("  bytes / param   %8.4f" % fp["bytes_per_param"])
    print("  purely 2-bit    NO (%s)" % fp["note"])
    print()
    print("-- sections --")
    for rec in manifest["sections"]:
        print("  %-26s off=%-8d len=%-9d %s"
              % (rec["name"], int(rec["offset"]), int(rec["length"]), rec["dtype"]))
    print()
    print("-- artefacts --")
    print("  container : %s (%d bytes)" % (out_path, file_size))
    print("  c header  : %s (%d bytes)" % (header_path, len(text.encode("ascii"))))
    print()

    failures = 0
    for name, ok, detail in results:
        if not ok:
            failures += 1
        print("%s: %s%s" % ("PASS" if ok else "FAIL", name,
                           (" (%s)" % detail) if detail else ""))
    print("SELFTEST %s: %d/%d checks"
          % ("PASS" if failures == 0 else "FAIL", len(results) - failures, len(results)))
    print("kernel support: %s" % manifest["kernel_support"])
    return 0 if failures == 0 else 1


# ---------------------------------------------------------------------------
# Golden artefact (deterministic, committed reference for the Rust loader)
# ---------------------------------------------------------------------------

#: Seed of the GLOBAL CPU RNG used to initialise the golden student.
GOLDEN_SEED = 1337
#: Seed of the EXPLICIT torch.Generator that draws the golden token ids.
GOLDEN_ID_SEED = 90210
#: Shape of the reference logits: [B, T, vocab].
GOLDEN_BATCH = 1
GOLDEN_SEQ = 5

#: The TINY golden student (documented in the module docstring).
GOLDEN_DENSE: Dict[str, Any] = {
    "hidden_size": 32,
    "num_layers": 1,
    "num_heads": 2,
    "num_kv_heads": 1,
    "head_dim": 16,
    "intermediate_size": 64,
    "vocab_size": 64,
    "rms_eps": 1e-5,
    "rope_theta": bf.DEFAULT_ROPE_THETA,
    "tie_embeddings": False,
}
GOLDEN_STUDENT: Dict[str, Any] = {
    "num_shared_blocks": 3,
    "num_experts": 2,
    "top_k": 1,
    "step_embeddings": 3,
    "use_act_halting": True,
    "act_threshold": bf.DEFAULT_ACT_THRESHOLD,
    "lowrank_rank": 0,
}

#: The exact command that regenerates the golden artefacts.
GOLDEN_REGENERATE = "python tools/export_no_std.py --golden"


def _golden_student() -> Any:
    """Build the TINY golden student with FULLY DETERMINISTIC weights.

    DETERMINISM CONTRACT:
      1. ``torch.manual_seed(GOLDEN_SEED)`` is called immediately BEFORE the
         constructor.  The student module initialises every parameter with
         ``nn.init.normal_`` / ``torch.randn`` and that API has NO generator
         argument, so the GLOBAL CPU RNG is the only source of randomness
         and the seed pins it.
      2. The constructor is the ONLY consumer of the global RNG and it
         consumes it in the same order every time, so the weights are a pure
         function of (GOLDEN_SEED, GOLDEN_DENSE, GOLDEN_STUDENT, the student
         module source, the torch version).
      3. NO RNG at export time: ``export_bfz`` only READS the weights (the
         trits are a deterministic function of them).  ``golden()`` proves
         it by fingerprinting the global RNG state around the forward and
         around the export.
      4. The input token ids come from an EXPLICIT torch.Generator
         (GOLDEN_ID_SEED), never from the global RNG.
    """
    torch.manual_seed(GOLDEN_SEED)
    dense = bf.DenseFalconConfig(**GOLDEN_DENSE)
    scfg = bf.StudentConfig(dense=dense, **GOLDEN_STUDENT)
    model = bf.BitFuzzyRecurrent(scfg)
    model.eval()
    return model


def _rng_fingerprint() -> bytes:
    """Fingerprint of the global CPU RNG state (proves 'no RNG at export')."""
    return bytes(torch.random.get_rng_state().numpy().tobytes())


def golden() -> Dict[str, Any]:
    """Generate the deterministic golden artefacts (container + ref logits).

    Writes (next to this script, i.e. in tools/):
      golden_bfz.bin        the model_type=3 container
      golden_bfz.ref.f32    the REFERENCE LOGITS of the SOURCE student
      golden_bfz.meta.json  config + ids + sha256 + regenerate command

    The reference logits are the SOURCE student's logits, NOT the rebuilt
    student's: the Rust loader has to reproduce them from the CONTAINER
    alone, so the golden must not be computed from a lossy round-trip.
    """
    tools_dir = Path(__file__).resolve().parent
    container_path = tools_dir / "golden_bfz.bin"
    ref_path = tools_dir / "golden_bfz.ref.f32"
    meta_path = tools_dir / "golden_bfz.meta.json"

    model = _golden_student()
    vocab = int(GOLDEN_DENSE["vocab_size"])
    ids = torch.randint(0, vocab, (GOLDEN_BATCH, GOLDEN_SEQ),
                        generator=torch.Generator().manual_seed(GOLDEN_ID_SEED))

    rng_before = _rng_fingerprint()
    with torch.no_grad():
        logits, steps = model(ids)
    rng_after_forward = _rng_fingerprint()

    manifest = export_bfz(model, container_path)
    rng_after_export = _rng_fingerprint()

    # The reference logits come from the SOURCE student.
    ref = np.ascontiguousarray(
        logits.detach().to(torch.float32).cpu().numpy(), dtype="<f4").tobytes()
    _require(len(ref) == 4 * GOLDEN_BATCH * GOLDEN_SEQ * vocab,
             "reference logits must be [B, T, vocab] f32")
    ref_path.write_bytes(ref)

    container_bytes = container_path.read_bytes()
    sha_container = hashlib.sha256(container_bytes).hexdigest()
    sha_ref = hashlib.sha256(ref).hexdigest()

    meta = {
        "artifact": "golden_bfz",
        "purpose": ("committed deterministic reference for the model_type=3 "
                    "(BitFuzzy recurrent MoE) container: the Rust loader test "
                    "reads golden_bfz.bin and compares against "
                    "golden_bfz.ref.f32"),
        "container": {
            "path": "tools/golden_bfz.bin",
            "bytes": len(container_bytes),
            "sha256": sha_container,
        },
        "reference_logits": {
            "path": "tools/golden_bfz.ref.f32",
            "bytes": len(ref),
            "sha256": sha_ref,
            "format": ("raw little-endian f32, row-major [B, T, vocab], NO "
                       "header bytes (the shape is documented here and in the "
                       "code, not in the file)"),
            "batch": GOLDEN_BATCH,
            "seq": GOLDEN_SEQ,
            "vocab": vocab,
            "floats": len(ref) // 4,
            "produced_by": "the SOURCE student (not the rebuilt one)",
        },
        "config": {
            "dense": dict(GOLDEN_DENSE),
            "student": dict(GOLDEN_STUDENT),
            "max_seq": int(model.max_seq),
        },
        "container_config": manifest["config"],
        "input_token_ids": ids.tolist(),
        "act_halting": {
            "enabled": bool(GOLDEN_STUDENT["use_act_halting"]),
            "act_threshold": float(model.act_threshold),
            "halt_center": float(model.halt_center.detach().reshape(-1)[0]),
            "halt_width": float(model.halt_width.detach().reshape(-1)[0]),
            "steps_used": int(steps),
        },
        "determinism": {
            "seed": GOLDEN_SEED,
            "id_seed": GOLDEN_ID_SEED,
            "method": ("torch.manual_seed(seed) before the constructor (the "
                       "student API has no generator argument); the input ids "
                       "use an explicit torch.Generator; no RNG is consumed at "
                       "forward/export time (proved by the RNG fingerprint)"),
            "rng_untouched_at_forward": rng_before == rng_after_forward,
            "rng_untouched_at_export": rng_before == rng_after_export,
            "torch": torch.__version__,
            "numpy": np.__version__,
            "student_module_sha256": hashlib.sha256(
                Path(bf.__file__).read_bytes()).hexdigest(),
        },
        "regenerate": GOLDEN_REGENERATE,
    }
    meta_path.write_text(json.dumps(meta, indent=2) + "\n", encoding="ascii")

    print("== golden artefacts (model_type=3, deterministic) ==")
    print("container      : tools/golden_bfz.bin (%d bytes)"
          % len(container_bytes))
    print("  sha256       : %s" % sha_container)
    print("reference      : tools/golden_bfz.ref.f32 (%d bytes, "
          "f32 LE [B=%d, T=%d, vocab=%d])"
          % (len(ref), GOLDEN_BATCH, GOLDEN_SEQ, vocab))
    print("  sha256       : %s" % sha_ref)
    print("meta           : tools/golden_bfz.meta.json")
    print("seed           : %d (weights) / %d (token ids)"
          % (GOLDEN_SEED, GOLDEN_ID_SEED))
    print("token ids      : %s" % ids.tolist())
    print("act halting   : on, threshold=%.3f, steps_used=%d"
          % (float(model.act_threshold), int(steps)))
    print("rng untouched  : forward=%s export=%s"
          % (rng_before == rng_after_forward, rng_before == rng_after_export))
    print("regenerate     : %s" % GOLDEN_REGENERATE)

    # Informational: does the CONTAINER alone reproduce the source logits?
    # (the golden artefacts are already on disk at this point)
    try:
        rebuilt = rebuild_student_from_bfz(container_path)
        rebuilt.eval()
        with torch.no_grad():
            new_logits, new_steps = rebuilt(ids)
        delta = float(torch.max(torch.abs(logits - new_logits)))
        scale = max(float(torch.max(torch.abs(logits))), 1e-12)
        print("container parity: max|delta|=%.3e rel=%.3e "
              "(source steps=%d, rebuilt steps=%d)"
              % (delta, delta / scale, int(steps), int(new_steps)))
    except Exception as exc:  # informational only: the artefacts are written
        print("container parity: UNAVAILABLE (%s)" % exc)
    return {"container": str(container_path), "reference": str(ref_path),
            "meta": str(meta_path), "sha256_container": sha_container,
            "sha256_reference": sha_ref}


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv: Optional[Sequence[str]] = None) -> int:
    ap = argparse.ArgumentParser(
        description="Serialise a BitFuzzy recurrent MoE student into the "
                    "model_type=3 bare-metal container (.bfz) + a C header.")
    ap.add_argument("--selftest", action="store_true",
                    help="run the built-in self test (tiny synthetic student)")
    ap.add_argument("--golden", action="store_true",
                    help="generate the deterministic committed golden "
                         "artefacts (tools/golden_bfz.bin + "
                         "tools/golden_bfz.ref.f32 + "
                         "tools/golden_bfz.meta.json)")
    ap.add_argument("--ckpt", default=None,
                    help="student checkpoint saved by tools/train_distill.py "
                         "(save_bitfuzzy)")
    ap.add_argument("--out", default=None, help="output container path")
    ap.add_argument("--c-header", default=None, help="output C header path")
    ap.add_argument("--embed-encoding", default="auto",
                    choices=["auto", "f32", "ternary", "q6k"],
                    help="encoding of the embedding/unembedding matrices "
                         "(auto = f32 exact for a real-valued student). "
                         "NOTE: ternary/q6k are LOSSY: the rebuilt student "
                         "will NOT match the source logits and --verify "
                         "reports FAIL (that is the honest outcome)")
    ap.add_argument("--json", action="store_true", help="print the manifest as JSON")
    ap.add_argument("--verify", action="store_true",
                    help="rebuild the student from the container and compare "
                         "the logits with the source checkpoint")
    args = ap.parse_args(list(argv) if argv is not None else None)

    if args.selftest:
        return selftest()

    if args.golden:
        golden()
        return 0

    if not args.ckpt:
        ap.print_help()
        return 2

    manifest = export_bfz(args.ckpt, args.out, embed_encoding=args.embed_encoding)
    if args.c_header:
        emit_c_header(args.c_header, manifest)

    parity = None
    if args.verify:
        parity = verify_parity(args.ckpt, args.out)
        print("== parity (container vs source checkpoint) ==")
        print("  max|delta logits| : %.3e   rel: %.3e   (tolerance %.0e)"
              % (parity["max_abs_delta"], parity["rel_delta"], parity["tolerance"]))
        print("  recurrent steps   : source %d, rebuilt %d"
              % (parity["src_steps"], parity["rebuilt_steps"]))
        print("  verdict           : %s" % ("PASS" if parity["ok"] else "FAIL"))
        print()
    if args.json:
        printable = dict(manifest)
        printable["routing"] = {
            k: (v.tolist() if hasattr(v, "tolist") else v)
            for k, v in manifest["routing"].items()
        }
        printable["masks"] = [
            {k: (v.tolist() if hasattr(v, "tolist") else v) for k, v in m.items()}
            for m in manifest["masks"]
        ]
        print(json.dumps(printable, indent=2, default=str))
    else:
        print(format_report(manifest))
    if args.c_header:
        print("\nC header written: %s" % args.c_header)
    if parity is not None and not parity["ok"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

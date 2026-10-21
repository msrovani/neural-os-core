#!/usr/bin/env python3
"""BitFuzzy recurrent MoE student + faithful dense Falcon3 teacher.

HOST-SIDE RESEARCH CODE. Read the honesty notes before trusting any number:

  * NO TRAINING HAS BEEN RUN with this module. Weights are randomly
    initialized unless you load them (load_dense_from_safetensors /
    load_bitfuzzy). Any accuracy claim about the student is UNKNOWN.
  * ZERO-FPU EXECUTION IS NOT IMPLEMENTED. The fuzzy routing math is written
    so that it is EXPRESSIBLE in fixed point (integer centers/widths/scores,
    see BitFuzzyMoE.quantize_routing_fixed_point), but no integer kernel,
    no integer inference loop and no FPU-free runtime exist yet.
  * The ternarization is ABSMEAN-BASED (per-output-row absmean over the input
    dimension, BitNet convention) with NO QAT CALIBRATION: no calibrated
    scale, no clipping search, no per-channel scale. The row scales are
    recomputed from the live weights on every forward.
  * DenseFalcon3 is a faithful DENSE re-implementation of Falcon3 in PyTorch
    (teacher for distillation + layer-angular analysis). It is NOT the kernel
    and it is not bit-exact with the kernel.

Canonical Falcon3-3B shapes (source of truth = the repo converters):
  tools/convert_falcon3_to_v6.py : FALCON_HIDDEN=3072, FALCON_LAYERS=22,
      FALCON_HEADS=12, FALCON_KV_HEADS=4, FALCON_INTERMEDIATE=9216,
      FALCON_VOCAB=131072, FALCON_ROPE_THETA=1000042.0, FALCON_TIE=False
  tools/convert_falcon3_bitnet.py : head_dim = cfg["head_dim"] (256 for the
      3B), rope_theta default 1000042.0
  => hidden 3072, 22 layers, 12 heads, 4 kv heads, head_dim 256,
     intermediate 9216, vocab 131072, rope_theta 1000042.0, untied lm_head.

RoPE layout: the kernel uses the INTERLEAVED convention for Falcon
(crates/cortex/src/cortex.rs::rope_apply_heads: pairs (2d, 2d+1) rotated with
cos/sin[d], inv_freq = theta ** (-2d/head_dim)). That is the default here.
The "half" (rotate_half) layout is available for the 2B4T path
(rope_apply_heads_half) but is NOT the Falcon3 convention.
"""

from __future__ import annotations

import argparse
import math
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple

import torch
import torch.nn as nn
import torch.nn.functional as F

# --------------------------------------------------------------------------
# Constants
# --------------------------------------------------------------------------

#: Falcon3-3B-1.58bit rope theta (repo converters: FALCON_ROPE_THETA).
DEFAULT_ROPE_THETA: float = 1000042.0

#: Zero deadzone of the ternarizer, as a fraction of the row absmean.
#: |w| <= ZERO_DEADZONE_RATIO * absmean(|w| row|) -> trit 0.
ZERO_DEADZONE_RATIO: float = 0.05

#: Temperature of the fuzzy straight-through estimator (mu = tanh(w / temp)).
DEFAULT_TEMP: float = 0.1

#: Default center of the ACT halting membership: the confidence at which a
#: sample counts as "done". The confidence is SCALE FREE (see
#: BitFuzzyRecurrent.halt_confidence), so this is a dimensionless number in
#: (0, 1]: 1.0 means "the recurrent step no longer changes the state".
DEFAULT_HALT_CENTER: float = 0.8

#: Default edge width of the ACT halting membership (the fuzzy edge).
DEFAULT_HALT_WIDTH: float = 0.05

#: Default ACT halting threshold: a sample halts when the fuzzy membership
#: of the confidence reaches this value.
DEFAULT_ACT_THRESHOLD: float = 0.9

#: Floor for the routing modulation so a zero ternary projection cannot zero
#: the whole router (that would make top-k selection arbitrary).
ROUTE_EPS: float = 1e-3

#: Floor of the fuzzy membership before the log-domain product T-norm.
MU_EPS: float = 1e-6

FALCON3_3B: Dict[str, Any] = {
    "hidden_size": 3072,
    "num_layers": 22,
    "num_heads": 12,
    "num_kv_heads": 4,
    "head_dim": 256,
    "intermediate_size": 9216,
    "vocab_size": 131072,
    "rope_theta": 1000042.0,
    "tie_embeddings": False,
}


# --------------------------------------------------------------------------
# Configs
# --------------------------------------------------------------------------


@dataclass
class DenseFalconConfig:
    """Shape contract of the dense Falcon3 teacher.

    Defaults for rope_theta follow the repo converters (1000042.0 for
    Falcon3-*-1.58bit), not the 10000.0 of the Llama-1 family.
    """

    hidden_size: int
    num_layers: int
    num_heads: int
    num_kv_heads: int
    head_dim: int
    intermediate_size: int
    vocab_size: int
    rms_eps: float = 1e-5
    rope_theta: float = DEFAULT_ROPE_THETA
    tie_embeddings: bool = True


@dataclass
class StudentConfig:
    """Config of the BitFuzzy recurrent student.

    num_shared_blocks: number of recurrent steps K over ONE weight-shared
        block (recurrent depth, not K independent blocks).
    num_experts / top_k: fuzzy-gated sparse MoE shape.
    step_embeddings: number of rows of the ternary step-embedding pool.
        The recurrent step t uses pool row (t % step_embeddings); when
        step_embeddings == num_shared_blocks the pool IS the per-block
        table of shape [num_shared_blocks, hidden].
    use_act_halting: enable the ACT-style early stop.
    act_threshold: the ACT halting threshold: a sample halts when the fuzzy
        membership of the confidence reaches this value (default 0.9). It is
        tunable at runtime with BitFuzzyRecurrent.set_act_threshold() and it
        is stored in the config so save_bitfuzzy/load_bitfuzzy round-trips it.
    lowrank_rank: rank of the optional real-valued low-rank correction
        added to the ternary weights (0 = off).
    """

    dense: DenseFalconConfig
    num_shared_blocks: int = 5
    num_experts: int = 8
    top_k: int = 2
    step_embeddings: int = 16
    use_act_halting: bool = True
    act_threshold: float = DEFAULT_ACT_THRESHOLD
    lowrank_rank: int = 0


# --------------------------------------------------------------------------
# Ternary core: fuzzy STE + packing
# --------------------------------------------------------------------------


def _row_absmean(w: torch.Tensor) -> torch.Tensor:
    """Per-output-row absmean over the LAST dimension (BitNet convention).

    The scale is DETACHED on purpose: the only gradient path of the ternary
    weight is the fuzzy STE (d mu / d w). A live scale would inject a second
    gradient path and silently change the optimizer's effective step size.
    """
    return w.detach().abs().mean(dim=-1, keepdim=True).clamp_min(1e-8)


def _broadcast_row_scale(scale: torch.Tensor, rank: int) -> torch.Tensor:
    """Reshape a [out, 1] scale so it broadcasts over the LAST dim only.

    A plain [out, 1] scale broadcasts against [N, out] but NOT against
    [B, T, out] (it would align with the T axis), so the scale is reshaped
    to [1, ..., 1, out] for the requested rank.
    """
    return scale.reshape(-1).reshape((1,) * (rank - 1) + (-1,))


def fuzzy_quantize(w: torch.Tensor, temp: float = DEFAULT_TEMP) -> torch.Tensor:
    """Ternarize ``w`` to {-1, 0, +1} in the forward pass, with a fuzzy STE.

    Forward: hard trits. A weight is 0 when |w| <= ZERO_DEADZONE_RATIO times
    the row absmean (the zero deadzone), +1 when w > 0, -1 when w < 0.

    Backward: the derivative of the hard sign is replaced by the derivative
    of the smooth membership mu(w) = tanh(w / temp). Concretely the returned
    tensor is ``hard.detach() + (mu - mu.detach())``: its value is exactly
    the hard trits (the two mu terms cancel bit-exactly in the forward) while
    its gradient is d mu / d w = (1 - tanh(w/temp)^2) / temp.

    This is a SMOOTH SURROGATE of the standard straight-through estimator,
    not a different arithmetic: the forward arithmetic is identical to the
    hard quantizer, only the derivative is replaced.

    The per-output-row absmean scale is NOT applied here (the caller owns
    it); this function returns dimensionless trits.
    """
    if temp <= 0.0:
        raise ValueError("temp must be > 0")
    dead = ZERO_DEADZONE_RATIO * _row_absmean(w)
    hard = torch.sign(w) * (w.abs() > dead).to(w.dtype)
    mu = torch.tanh(w / temp)
    return hard.detach() + (mu - mu.detach())


def ternary_pack(w: torch.Tensor) -> torch.Tensor:
    """Pack a tensor of trits in {-1, 0, +1} into 2-bit codes, 4 per byte.

    ORDER (identical to tools/bitnet_writer.py::pack_ternary, the repo .bitnet
    v6 layout):
      * the source is flattened ROW-MAJOR (C order): flat index
        f = row * in_features + col;
      * trit f lives in byte f // 4, in bits (2 * (f % 4)) ..
        (2 * (f % 4) + 1) of that byte (least significant bits first);
      * code 00 = 0, 01 = +1, 10 = -1, 11 = unused (decoded as 0).
    A trailing partial group is zero padded.

    NOTE: this is NOT the HuggingFace BitNet packing, which groups the four
    trits of a byte along the OUTPUT dimension (byte (r, c) holds output rows
    r*4 .. r*4+3 for input column c). See _unpack_hf_ternary.
    """
    flat = w.detach().reshape(-1).to(torch.int8)
    codes = torch.zeros(flat.numel(), dtype=torch.uint8)
    codes[flat > 0] = 0b01
    codes[flat < 0] = 0b10
    pad = (-codes.numel()) % 4
    if pad:
        codes = torch.cat([codes, torch.zeros(pad, dtype=torch.uint8)])
    groups = codes.view(-1, 4)
    packed = groups[:, 0] | (groups[:, 1] << 2) | (groups[:, 2] << 4) | (groups[:, 3] << 6)
    return packed.to(torch.uint8)


def ternary_unpack(packed: torch.Tensor, shape: Tuple[int, int]) -> torch.Tensor:
    """Inverse of ternary_pack. Returns int8 trits of the given 2D shape."""
    out_features, in_features = int(shape[0]), int(shape[1])
    raw = packed.to(torch.uint8).reshape(-1)
    codes = torch.stack([(raw >> (2 * i)) & 3 for i in range(4)], dim=1)  # [nbytes, 4]
    lut = torch.tensor([0, 1, -1, 0], dtype=torch.int8)
    trits = lut[codes.long()].reshape(-1)[: out_features * in_features]
    return trits.reshape(out_features, in_features).contiguous()


def _unpack_hf_ternary(u8: torch.Tensor, out_features: int, in_features: int) -> torch.Tensor:
    """Unpack the HuggingFace BitNet ternary packing into float trits.

    Layout (same as tools/convert_falcon3_bitnet.py::unpack_hf_oi): the
    source is [out_features // 4, in_features] uint8 and byte (r, c) holds
    the output rows r*4 .. r*4+3 for the input column c, trit i of a byte in
    bits 2i..2i+1 with 00=0, 01=+1, 10=-1.

    Used by load_dense_from_safetensors when the checkpoint stores the 1.58
    bit trits (uint8) instead of full precision weights.
    """
    if u8.ndim == 1:
        u8 = u8.reshape(out_features // 4, in_features)
    flat = u8.reshape(-1).to(torch.int64)
    codes = torch.stack([(flat >> (2 * i)) & 3 for i in range(4)], dim=1)  # [nbytes, 4]
    lut = torch.tensor([0, 1, -1, 0], dtype=torch.int8)
    out4 = out_features // 4
    trits = lut[codes.long()].view(out4, 4, in_features).transpose(1, 2).reshape(out4 * 4, in_features)
    return trits.reshape(out_features, in_features).contiguous().float()


# --------------------------------------------------------------------------
# Norm + RoPE
# --------------------------------------------------------------------------


class RMSNorm(nn.Module):
    """Pre-norm RMSNorm: x * rsqrt(mean(x^2) + eps) * weight.

    Matches the Falcon3/HF convention (weight applied AFTER the
    normalization, no mean subtraction, no bias).
    """

    def __init__(self, dim: int, eps: float = 1e-5) -> None:
        super().__init__()
        self.eps = float(eps)
        self.weight = nn.Parameter(torch.ones(dim))

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        dtype = x.dtype
        x32 = x.float()
        x32 = x32 * torch.rsqrt(x32.pow(2).mean(dim=-1, keepdim=True) + self.eps)
        return (x32 * self.weight.float()).to(dtype)


class RotaryEmbedding(nn.Module):
    """Rotary position embedding with precomputed cos/sin tables.

    layout="interleaved" (default): pairs (2d, 2d+1) are rotated with
    (cos[d], sin[d]) and inv_freq = theta ** (-2d / head_dim). This is the
    convention the kernel uses for Falcon (act_type 0,
    crates/cortex/src/cortex.rs::rope_apply_heads).

    layout="half": the first and second halves are rotated against each
    other (rotate_half, the 2B4T/BitNet-HF convention,
    cortex.rs::rope_apply_heads_half).

    The forward accepts x of shape [..., T, head_dim] and returns the same
    shape. seq_offset shifts the position of the first token (for decoding
    with a KV cache).
    """

    def __init__(
        self,
        head_dim: int,
        max_seq: int = 4096,
        theta: float = DEFAULT_ROPE_THETA,
        layout: str = "interleaved",
    ) -> None:
        super().__init__()
        if head_dim % 2 != 0:
            raise ValueError("head_dim must be even")
        if layout not in ("interleaved", "half"):
            raise ValueError("layout must be 'interleaved' or 'half'")
        self.head_dim = int(head_dim)
        self.max_seq = int(max_seq)
        self.theta = float(theta)
        self.layout = layout
        half = head_dim // 2
        inv_freq = torch.pow(
            torch.tensor(float(theta), dtype=torch.float32),
            -2.0 * torch.arange(half, dtype=torch.float32) / float(head_dim),
        )
        pos = torch.arange(max_seq, dtype=torch.float32)
        freqs = pos[:, None] * inv_freq[None, :]
        self.register_buffer("cos", freqs.cos(), persistent=False)
        self.register_buffer("sin", freqs.sin(), persistent=False)

    def forward(self, x: torch.Tensor, seq_offset: int = 0) -> torch.Tensor:
        if x.shape[-1] != self.head_dim:
            raise ValueError(f"expected last dim {self.head_dim}, got {x.shape[-1]}")
        seq_len = x.shape[-2]
        end = min(seq_offset + seq_len, self.max_seq)
        if end <= seq_offset:
            return x
        cos = self.cos[seq_offset:end].to(x.dtype)
        sin = self.sin[seq_offset:end].to(x.dtype)
        if seq_len > end - seq_offset:
            # Beyond the precomputed window: pad by repeating the last row.
            pad = seq_len - (end - seq_offset)
            cos = torch.cat([cos, cos[-1:].expand(pad, cos.shape[-1])], dim=0)
            sin = torch.cat([sin, sin[-1:].expand(pad, sin.shape[-1])], dim=0)
        half = self.head_dim // 2
        if self.layout == "interleaved":
            x1 = x[..., 0::2]
            x2 = x[..., 1::2]
            o1 = x1 * cos - x2 * sin
            o2 = x1 * sin + x2 * cos
            return torch.stack([o1, o2], dim=-1).flatten(-2)
        x1 = x[..., :half]
        x2 = x[..., half:]
        o1 = x1 * cos - x2 * sin
        o2 = x1 * sin + x2 * cos
        return torch.cat([o1, o2], dim=-1)


# --------------------------------------------------------------------------
# Ternary layers
# --------------------------------------------------------------------------


class BitLinear(nn.Module):
    """Ternary (1.58-bit) linear layer with the fuzzy STE.

    The effective weight is W_eff[m, i] = row_scale[m] * trit[m, i] with
    row_scale[m] = absmean(|W[m, :]|) (BitNet convention, recomputed from the
    live weights). The forward is y = (x @ W_eff.T), implemented as a plain
    linear over the trits followed by the per-row scale.

    Gradient: only through the fuzzy membership mu(w) = tanh(w / temp).

    lowrank_rank > 0 adds a real-valued low-rank correction
    (x @ B.T) @ A.T with A init ~ N(0, 0.02) and B init 0, so the layer
    starts exactly at the ternary solution and can recover capacity.
    """

    def __init__(
        self,
        in_features: int,
        out_features: int,
        bias: bool = False,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
    ) -> None:
        super().__init__()
        self.in_features = int(in_features)
        self.out_features = int(out_features)
        self.temp = float(temp)
        self.lowrank_rank = int(lowrank_rank)
        self.weight = nn.Parameter(torch.empty(out_features, in_features))
        nn.init.normal_(self.weight, std=0.02)
        if bias:
            self.bias = nn.Parameter(torch.zeros(out_features))
        else:
            self.register_parameter("bias", None)
        if self.lowrank_rank > 0:
            self.lora_a = nn.Parameter(torch.randn(out_features, self.lowrank_rank) * 0.02)
            self.lora_b = nn.Parameter(torch.zeros(self.lowrank_rank, in_features))
        else:
            self.register_parameter("lora_a", None)
            self.register_parameter("lora_b", None)

    def row_scale(self) -> torch.Tensor:
        """Per-output-row absmean scale, shape [out, 1] (detached)."""
        return _row_absmean(self.weight)

    def ternary_weight(self) -> torch.Tensor:
        """Trits with the fuzzy STE attached (forward values in {-1, 0, 1})."""
        return fuzzy_quantize(self.weight, self.temp)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        y = F.linear(x, self.ternary_weight())
        y = y * _broadcast_row_scale(self.row_scale(), y.dim())
        if self.lora_a is not None:
            y = y + F.linear(F.linear(x, self.lora_b), self.lora_a)
        if self.bias is not None:
            y = y + self.bias
        return y


class SugenoBitLinear(nn.Module):
    """Sugeno-style ternary layer: ternary W_bar [out, in] and ternary mask P [in].

    The mask is per INPUT FEATURE and takes values in {+1, -1, 0}: it selects
    h[i], -h[i] or 0 for every input feature i. The layer computes

        y[m] = sum_i W_bar[m, i] * (P[i] o h[i])

    where o is the element-wise product with the ternary mask. This is the
    crisp (Sugeno) implication of a fuzzy rule set: the mask encodes the
    antecedent, W_bar the consequent aggregation.

    masked_activations(h) exposes the concrete +/-/0 selection so an integer
    kernel can implement it without any floating point.
    """

    def __init__(
        self,
        in_features: int,
        out_features: int,
        bias: bool = False,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
    ) -> None:
        super().__init__()
        self.in_features = int(in_features)
        self.out_features = int(out_features)
        self.temp = float(temp)
        self.lowrank_rank = int(lowrank_rank)
        self.weight = nn.Parameter(torch.empty(out_features, in_features))
        nn.init.normal_(self.weight, std=0.02)
        self.mask = nn.Parameter(torch.randn(in_features) * 0.5)
        if bias:
            self.bias = nn.Parameter(torch.zeros(out_features))
        else:
            self.register_parameter("bias", None)
        if self.lowrank_rank > 0:
            self.lora_a = nn.Parameter(torch.randn(out_features, self.lowrank_rank) * 0.02)
            self.lora_b = nn.Parameter(torch.zeros(self.lowrank_rank, in_features))
        else:
            self.register_parameter("lora_a", None)
            self.register_parameter("lora_b", None)

    def row_scale(self) -> torch.Tensor:
        return _row_absmean(self.weight)

    def ternary_weight(self) -> torch.Tensor:
        return fuzzy_quantize(self.weight, self.temp)

    def mask_trits(self) -> torch.Tensor:
        """The ternary mask P as trits in {-1, 0, 1}, shape [in].

        The deadzone is computed against the absmean of the whole mask
        vector, so a feature can be pruned to 0 while the others stay.
        """
        return fuzzy_quantize(self.mask.view(1, -1), self.temp).view(-1)

    def masked_activations(self, h: torch.Tensor) -> torch.Tensor:
        """Concrete selection: h, -h or 0 per input feature."""
        return h * self.mask_trits()

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        y = F.linear(self.masked_activations(x), self.ternary_weight())
        y = y * _broadcast_row_scale(self.row_scale(), y.dim())
        if self.lora_a is not None:
            y = y + F.linear(F.linear(x, self.lora_b), self.lora_a)
        if self.bias is not None:
            y = y + self.bias
        return y


class _TernaryExpert(nn.Module):
    """One SwiGLU-shaped ternary expert (BitNet style)."""

    def __init__(
        self,
        hidden_size: int,
        ffn_size: int,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
        sugeno: bool = False,
    ) -> None:
        super().__init__()
        gate_cls = SugenoBitLinear if sugeno else BitLinear
        self.gate = gate_cls(hidden_size, ffn_size, temp=temp, lowrank_rank=lowrank_rank)
        self.up = BitLinear(hidden_size, ffn_size, temp=temp, lowrank_rank=lowrank_rank)
        self.down = BitLinear(ffn_size, hidden_size, temp=temp, lowrank_rank=lowrank_rank)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.down(F.silu(self.gate(x)) * self.up(x))


class BitFuzzyMoE(nn.Module):
    """Fuzzy-gated sparse MoE with a ternary routing matrix.

    Routing pipeline (per token):
      1. membership: per (expert, feature) fuzzy membership mu in [0, 1] of
         the input feature against a learned center and width (gaussian by
         default, triangular available).
      2. T-norm: the per-feature memberships are aggregated with a fuzzy
         AND. "product" (default) is the classical product T-norm,
         "min" is the minimum T-norm (Godel). The product is evaluated in
         the log domain; with product_normalized=True (default) it is the
         geometric mean, because the literal product underflows to 0 for
         more than ~128 features and would make top-k selection arbitrary.
      3. ternary routing matrix W_g [E, in]: the signed projection
         sum_i W_g[e, i] * x[i] modulates the score by its magnitude.
      4. top-k selection with normalized weights (softmax over the selected
         scores by default).

    The math is written so it can be expressed in fixed point (integer
    centers/widths/scores); see quantize_routing_fixed_point. The integer
    kernel does NOT exist yet.

    Attributes: tnorm ("product" | "min"), membership_kind ("gaussian" |
    "triangular"), weight_mode ("softmax" | "sum"), product_normalized,
    centers/widths [E, in], routing [E, in] (ternary W_g).
    """

    def __init__(
        self,
        hidden_size: int,
        ffn_size: int,
        num_experts: int = 8,
        top_k: int = 2,
        temp: float = DEFAULT_TEMP,
        tnorm: str = "product",
        product_normalized: bool = True,
        membership: str = "gaussian",
        lowrank_rank: int = 0,
        sugeno: bool = True,
        weight_mode: str = "softmax",
        score_temperature: float = 1.0,
    ) -> None:
        super().__init__()
        if tnorm not in ("product", "min"):
            raise ValueError("tnorm must be 'product' or 'min'")
        if membership not in ("gaussian", "triangular"):
            raise ValueError("membership must be 'gaussian' or 'triangular'")
        if weight_mode not in ("softmax", "sum"):
            raise ValueError("weight_mode must be 'softmax' or 'sum'")
        self.hidden_size = int(hidden_size)
        self.ffn_size = int(ffn_size)
        self.num_experts = int(num_experts)
        self.top_k = int(min(top_k, num_experts))
        self.temp = float(temp)
        self.tnorm = tnorm
        self.product_normalized = bool(product_normalized)
        self.membership_kind = membership
        self.weight_mode = weight_mode
        self.score_temperature = float(score_temperature)
        self.experts = nn.ModuleList(
            [
                _TernaryExpert(hidden_size, ffn_size, temp=temp, lowrank_rank=lowrank_rank, sugeno=sugeno)
                for _ in range(self.num_experts)
            ]
        )
        self.centers = nn.Parameter(torch.randn(num_experts, hidden_size) * 0.5)
        self.widths = nn.Parameter(torch.ones(num_experts, hidden_size))
        self.routing = nn.Parameter(torch.randn(num_experts, hidden_size) * 0.5)

    # -- routing math ------------------------------------------------------

    def membership(self, x: torch.Tensor) -> torch.Tensor:
        """Fuzzy membership of the input features: x [..., in] -> mu [E, ..., in]."""
        centers = self.centers
        widths = self.widths.abs().clamp_min(1e-3)
        # Insert the expert axis so [E, in] lines up with [1, ..., in].
        shape = (centers.shape[0],) + (1,) * (x.dim() - 1) + (centers.shape[1],)
        diff = x.unsqueeze(0) - centers.reshape(shape)
        w_e = widths.reshape(shape)
        if self.membership_kind == "triangular":
            mu = (1.0 - diff.abs() / w_e).clamp(0.0, 1.0)
        else:
            mu = torch.exp(-(diff * diff) / (2.0 * w_e * w_e))
        return mu.clamp(0.0, 1.0)

    def tnorm_scores(self, mu: torch.Tensor) -> torch.Tensor:
        """Aggregate per-feature memberships: mu [E, ..., in] -> scores [E, ...]."""
        if self.tnorm == "min":
            return mu.min(dim=-1).values
        logs = torch.log(mu.clamp_min(MU_EPS))
        total = logs.sum(dim=-1)
        if self.product_normalized:
            total = total / float(mu.shape[-1])
        return torch.exp(total)

    def routing_trits(self) -> torch.Tensor:
        """The ternary routing matrix W_g as trits in {-1, 0, 1}, shape [E, in]."""
        return fuzzy_quantize(self.routing, self.temp)

    def route_modulation(self, x: torch.Tensor) -> torch.Tensor:
        """Signed ternary projection per expert: x [..., in] -> mod [E, ...]."""
        g = self.routing_trits()
        return torch.einsum("ei,...i->e...", g, x)

    def scores(self, x: torch.Tensor) -> torch.Tensor:
        """Expert scores: x [..., in] -> scores [E, ...]."""
        flat = x.reshape(-1, x.shape[-1])
        base = self.tnorm_scores(self.membership(flat))
        mod = self.route_modulation(flat).abs() + ROUTE_EPS
        return (base * mod).reshape((self.num_experts,) + tuple(x.shape[:-1]))

    def route_info(self, x: torch.Tensor) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
        """Return (scores [E, ...], top_indices [k, ...], weights [k, ...])."""
        scores = self.scores(x)
        k = min(self.top_k, self.num_experts)
        topv, topi = torch.topk(scores, k, dim=0)
        if self.weight_mode == "softmax":
            weights = torch.softmax(topv / self.score_temperature, dim=0)
        else:
            weights = topv / topv.sum(dim=0, keepdim=True).clamp_min(1e-9)
        return scores, topi, weights

    # -- forward -----------------------------------------------------------

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        lead = tuple(x.shape[:-1])
        flat = x.reshape(-1, x.shape[-1])
        _, topi, weights = self.route_info(flat)
        k = topi.shape[0]
        outs = torch.stack([expert(flat) for expert in self.experts], dim=0)  # [E, N, out]
        sel = F.one_hot(topi, num_classes=self.num_experts).permute(2, 0, 1).to(x.dtype)
        mixed = torch.einsum("ekn,eno->nko", sel, outs)  # [N, k, out]
        mixed = mixed * weights.t().unsqueeze(-1)  # [N, k, 1]
        mixed = mixed.sum(dim=1)  # [N, out]
        return mixed.reshape(lead + (mixed.shape[-1],))

    # -- fixed point --------------------------------------------------------

    def quantize_routing_fixed_point(
        self, bits: int = 8, x: Optional[torch.Tensor] = None
    ) -> Dict[str, Any]:
        """Integer view of the router (see the module level function)."""
        return quantize_routing_fixed_point(self, bits=bits, x=x)


def quantize_routing_fixed_point(
    moe: BitFuzzyMoE, bits: int = 8, x: Optional[torch.Tensor] = None
) -> Dict[str, Any]:
    """Convert the fuzzy router of ``moe`` to fixed-point integers.

    Scheme: per feature (column) affine mapping of the centers and of the
    widths onto [0, 2^bits - 1]:
        q = round((v - offset) / scale),  v = q * scale + offset
    The routing matrix stays ternary (it is already 2 bits).

    When x is given, the T-norm base scores (in [0, 1] for both the product
    and the min norm) are also converted to integers of the same range.

    Fixed-point status (honest): the min T-norm maps to an exact integer
    min; the product T-norm needs a log-domain integer accumulator (or a
    saturating fixed-point multiply chain) and the membership itself needs
    an integer exp approximation or a lookup table. The math is EXPRESSIBLE
    in fixed point, but no integer kernel and no zero-FPU runtime exist yet.
    """
    if not 1 <= bits <= 31:
        raise ValueError("bits must be in [1, 31]")
    levels = (1 << bits) - 1
    centers = moe.centers.detach().float()
    widths = moe.widths.detach().float().abs()
    c_min = centers.amin(dim=0)
    c_max = centers.amax(dim=0)
    c_scale = ((c_max - c_min) / float(levels)).clamp_min(1e-12)
    centers_q = torch.round((centers - c_min) / c_scale).clamp(0, levels).to(torch.int32)
    w_max = widths.amax(dim=0)
    w_scale = (w_max / float(levels)).clamp_min(1e-12)
    widths_q = torch.round(widths / w_scale).clamp(0, levels).to(torch.int32)
    out: Dict[str, Any] = {
        "bits": int(bits),
        "levels": int(levels),
        "tnorm": moe.tnorm,
        "product_normalized": bool(moe.product_normalized),
        "membership": moe.membership_kind,
        "top_k": int(moe.top_k),
        "num_experts": int(moe.num_experts),
        "centers_q": centers_q,
        "centers_offset": c_min,
        "centers_scale": c_scale,
        "widths_q": widths_q,
        "widths_offset": torch.zeros_like(w_max),
        "widths_scale": w_scale,
        "routing_trits": moe.routing_trits().detach().to(torch.int8),
        "note": (
            "Fixed-point EXPRESSIBLE only. The integer kernel does not exist: "
            "min T-norm is an exact integer min, product T-norm needs a "
            "log-domain integer accumulator and the membership needs an "
            "integer exp or a LUT. Zero-FPU execution is NOT implemented."
        ),
    }
    if x is not None:
        flat = x.reshape(-1, x.shape[-1])
        base = moe.tnorm_scores(moe.membership(flat))
        out["scores_q"] = torch.round(base * float(levels)).clamp(0, levels).to(torch.int32)
        out["scores_shape"] = tuple(base.shape)
    return out


# --------------------------------------------------------------------------
# Dense Falcon3 teacher
# --------------------------------------------------------------------------


class FalconAttention(nn.Module):
    """Manual GQA attention (matmul + causal mask + softmax).

    scaled_dot_product_attention is deliberately NOT used: the explicit
    path is what can be exported to the kernel and what the layer-angular
    analysis needs to instrument.
    """

    def __init__(
        self,
        cfg: DenseFalconConfig,
        rope: RotaryEmbedding,
        ternary: bool = False,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
    ) -> None:
        super().__init__()
        self.cfg = cfg
        self.rope = rope
        self.num_heads = cfg.num_heads
        self.num_kv_heads = cfg.num_kv_heads
        self.head_dim = cfg.head_dim
        self.group_size = max(1, cfg.num_heads // max(1, cfg.num_kv_heads))
        q_dim = cfg.num_heads * cfg.head_dim
        k_dim = cfg.num_kv_heads * cfg.head_dim
        if ternary:
            def mk(i: int, o: int) -> nn.Module:
                return BitLinear(i, o, temp=temp, lowrank_rank=lowrank_rank)
        else:
            def mk(i: int, o: int) -> nn.Module:
                return nn.Linear(i, o, bias=False)
        # HF convention: nn.Linear(in, out) -> weight [out, in].
        self.q_proj = mk(cfg.hidden_size, q_dim)
        self.k_proj = mk(cfg.hidden_size, k_dim)
        self.v_proj = mk(cfg.hidden_size, k_dim)
        self.o_proj = mk(q_dim, cfg.hidden_size)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        bsz, seq, _ = x.shape
        q = self.q_proj(x).view(bsz, seq, self.num_heads, self.head_dim).transpose(1, 2)
        k = self.k_proj(x).view(bsz, seq, self.num_kv_heads, self.head_dim).transpose(1, 2)
        v = self.v_proj(x).view(bsz, seq, self.num_kv_heads, self.head_dim).transpose(1, 2)
        q = self.rope(q)
        k = self.rope(k)
        if self.group_size > 1:
            # GQA: kv head g serves the query heads [g*group, (g+1)*group).
            k = k.repeat_interleave(self.group_size, dim=1)
            v = v.repeat_interleave(self.group_size, dim=1)
        scale = 1.0 / math.sqrt(self.head_dim)
        scores = torch.matmul(q.float(), k.float().transpose(-1, -2)) * scale
        causal = torch.ones(seq, seq, dtype=torch.bool, device=x.device).tril()
        scores = scores.masked_fill(~causal, float("-inf"))
        probs = torch.softmax(scores, dim=-1).to(x.dtype)
        ctx = torch.matmul(probs, v)
        ctx = ctx.transpose(1, 2).reshape(bsz, seq, self.num_heads * self.head_dim)
        return self.o_proj(ctx)


class SwiGLUMLP(nn.Module):
    """Dense SwiGLU MLP: down(silu(gate(x)) * up(x))."""

    def __init__(
        self,
        hidden_size: int,
        intermediate_size: int,
        ternary: bool = False,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
    ) -> None:
        super().__init__()
        if ternary:
            def mk(i: int, o: int) -> nn.Module:
                return BitLinear(i, o, temp=temp, lowrank_rank=lowrank_rank)
        else:
            def mk(i: int, o: int) -> nn.Module:
                return nn.Linear(i, o, bias=False)
        self.gate_proj = mk(hidden_size, intermediate_size)
        self.up_proj = mk(hidden_size, intermediate_size)
        self.down_proj = mk(intermediate_size, hidden_size)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.down_proj(F.silu(self.gate_proj(x)) * self.up_proj(x))


class Falcon3Block(nn.Module):
    """One Falcon3 block: pre-norm attention + pre-norm MLP, both residual.

    The MLP is either the dense SwiGLU or the fuzzy MoE (use_moe=True).
    """

    def __init__(
        self,
        cfg: DenseFalconConfig,
        rope: Optional[RotaryEmbedding] = None,
        use_moe: bool = False,
        num_experts: int = 8,
        top_k: int = 2,
        ternary: bool = False,
        temp: float = DEFAULT_TEMP,
        lowrank_rank: int = 0,
    ) -> None:
        super().__init__()
        self.cfg = cfg
        self.rope = rope if rope is not None else RotaryEmbedding(cfg.head_dim, theta=cfg.rope_theta)
        self.input_layernorm = RMSNorm(cfg.hidden_size, cfg.rms_eps)
        self.post_attention_layernorm = RMSNorm(cfg.hidden_size, cfg.rms_eps)
        self.self_attn = FalconAttention(
            cfg, self.rope, ternary=ternary, temp=temp, lowrank_rank=lowrank_rank
        )
        if use_moe:
            self.mlp: nn.Module = BitFuzzyMoE(
                cfg.hidden_size,
                cfg.intermediate_size,
                num_experts=num_experts,
                top_k=top_k,
                temp=temp,
                lowrank_rank=lowrank_rank,
            )
        else:
            self.mlp = SwiGLUMLP(
                cfg.hidden_size,
                cfg.intermediate_size,
                ternary=ternary,
                temp=temp,
                lowrank_rank=lowrank_rank,
            )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        x = x + self.self_attn(self.input_layernorm(x))
        x = x + self.mlp(self.post_attention_layernorm(x))
        return x


class DenseFalcon3(nn.Module):
    """Full dense Falcon3 stack: embeddings + N blocks + final norm + lm_head.

    Numerically faithful to Falcon3 in the following sense: pre-norm RMSNorm
    with the configured eps, RoPE on q and k (interleaved layout, the
    convention the kernel uses for Falcon), GQA with the kv heads repeated
    over their query group, SwiGLU MLP (silu(gate) * up -> down), final
    RMSNorm and an untied or tied lm_head. No biases anywhere (the .bitnet
    v6 format has none).

    forward(input_ids) returns logits of shape [B, T, vocab_size].
    """

    def __init__(self, cfg: DenseFalconConfig, max_seq: int = 4096) -> None:
        super().__init__()
        self.cfg = cfg
        self.max_seq = int(max_seq)
        self.embed_tokens = nn.Embedding(cfg.vocab_size, cfg.hidden_size)
        nn.init.normal_(self.embed_tokens.weight, std=0.02)
        self.rope = RotaryEmbedding(cfg.head_dim, max_seq=max_seq, theta=cfg.rope_theta)
        self.layers = nn.ModuleList(
            [Falcon3Block(cfg, self.rope, use_moe=False, ternary=False) for _ in range(cfg.num_layers)]
        )
        self.norm = RMSNorm(cfg.hidden_size, cfg.rms_eps)
        self.lm_head = nn.Linear(cfg.hidden_size, cfg.vocab_size, bias=False)
        if cfg.tie_embeddings:
            self.lm_head.weight = self.embed_tokens.weight

    def forward(self, input_ids: torch.Tensor) -> torch.Tensor:
        h = self.embed_tokens(input_ids)
        for layer in self.layers:
            h = layer(h)
        return self.lm_head(self.norm(h))


# --------------------------------------------------------------------------
# BitFuzzy recurrent student
# --------------------------------------------------------------------------


class BitFuzzyRecurrent(nn.Module):
    """Recurrent student: K steps over ONE weight-shared Falcon3 block.

    Per recurrent step t:
        h <- h + gain[t] * P_step(t)      (ternary step embedding)
        h <- block(h)                      (shared block: ternary attention
                                           + fuzzy MoE MLP)

    ACT-style halting, with the STANDARD ACT semantics:

      * The output of a recurrent step is ALWAYS applied to the state,
        INCLUDING the step that triggers the halt. The halting decision only
        decides whether ANOTHER step is executed. Dropping the output of the
        halting step would take the shared block out of the logits and cut
        the gradient path to it (the trainer measured grad_block == 0 and
        the recurrence depth K > 1 became a no-op).
      * A sample that already halted keeps its state frozen while the rest
        of the batch keeps stepping, so the batch runs until every sample
        halted or the K steps are exhausted.
      * The confidence is the SCALE FREE relative size of the step update
        (see halt_confidence), mapped through a fuzzy membership (sigmoid by
        default) and compared against act_threshold.

    forward(input_ids) returns (logits [B, T, vocab], steps_used) where
    steps_used is the number of recurrent steps the batch actually ran
    (1 <= steps_used <= K). The per-sample step count is kept in
    last_steps_per_sample for the trainer lane.

    Tuning the halting (the recurrence must not collapse to a single step):
      * act_threshold (default 0.9, also in StudentConfig): a sample halts
        when the fuzzy membership reaches it. Raise it to let the recurrence
        run longer, lower it to stop earlier. Runtime:
        student.set_act_threshold(0.95); constructor:
        BitFuzzyRecurrent(cfg, act_threshold=0.95).
      * halt_center / halt_width (default 0.8 / 0.05): the membership center
        and edge width. They are learnable parameters: the hard decision is
        boolean, so they only receive a straight-through surrogate gradient
        (see forward). The ponder cost is the trainer's job.
    """

    def __init__(self, cfg: StudentConfig, max_seq: int = 4096,
                 act_threshold: Optional[float] = None) -> None:
        super().__init__()
        self.cfg = cfg
        d = cfg.dense
        self.max_seq = int(max_seq)
        self.embed_tokens = nn.Embedding(d.vocab_size, d.hidden_size)
        nn.init.normal_(self.embed_tokens.weight, std=0.02)
        self.rope = RotaryEmbedding(d.head_dim, max_seq=max_seq, theta=d.rope_theta)
        self.block = Falcon3Block(
            d,
            self.rope,
            use_moe=True,
            num_experts=cfg.num_experts,
            top_k=cfg.top_k,
            ternary=True,
            lowrank_rank=cfg.lowrank_rank,
        )
        # Ternary step-embedding pool (P_step).
        self.step_emb = nn.Parameter(torch.randn(max(1, cfg.step_embeddings), d.hidden_size) * 0.5)
        # Per-step gain: keeps the ternary step signal small at init
        # (the trits are +-1, which would otherwise dominate the state).
        self.step_gain = nn.Parameter(torch.full((max(1, cfg.step_embeddings),), 0.1))
        # ACT halting membership: sigmoid((conf - center) / width).
        self.halt_center = nn.Parameter(torch.tensor([DEFAULT_HALT_CENTER]))
        self.halt_width = nn.Parameter(torch.tensor([DEFAULT_HALT_WIDTH]))
        # The halting threshold is a plain settable float (not a parameter):
        # the halt decision is a hard comparison, so there is no gradient path
        # to it. The constructor argument wins over the config value.
        # The live value (self.act_threshold) is the source of truth; the
        # config keeps the value the model was built with, and save_bitfuzzy
        # persists the live one so a runtime tuning survives a round-trip.
        self.act_threshold = float(
            cfg.act_threshold if act_threshold is None else act_threshold
        )
        self.last_steps_per_sample: Optional[torch.Tensor] = None
        self.norm = RMSNorm(d.hidden_size, d.rms_eps)
        self.lm_head = nn.Linear(d.hidden_size, d.vocab_size, bias=False)
        if d.tie_embeddings:
            self.lm_head.weight = self.embed_tokens.weight

    # -- helpers ------------------------------------------------------------

    def step_vector(self, t: int) -> torch.Tensor:
        """P_step(t): the ternary step embedding of recurrent step t.

        Returns shape [1, 1, hidden] so it broadcasts over [B, T, hidden].
        """
        idx = t % self.step_emb.shape[0]
        vec = fuzzy_quantize(self.step_emb[idx].view(1, -1), DEFAULT_TEMP).view(1, 1, -1)
        return vec * self.step_gain[idx]

    def set_act_threshold(self, value: float) -> None:
        """Set the ACT halting threshold at runtime.

        A sample halts when the fuzzy membership of the confidence reaches
        this value. Default: StudentConfig.act_threshold (0.9). Raise it to
        let the recurrence run longer steps, lower it to stop earlier.

        Only this model is changed (the shared config object is left alone),
        and save_bitfuzzy persists the live value, so a threshold tuned here
        survives a save / load round-trip.
        """
        self.act_threshold = float(value)

    def halt_membership(self, conf: torch.Tensor) -> torch.Tensor:
        """Fuzzy membership of the confidence, in [0, 1]."""
        width = self.halt_width.abs().clamp_min(1e-3)
        return torch.sigmoid((conf - self.halt_center) / width)

    def halt_confidence(self, h_new: torch.Tensor, h_prev: torch.Tensor) -> torch.Tensor:
        """Scale free halting confidence, one value per batch element.

            conf = 1 / (1 + rel)   with   rel = RMS(h_new - h_prev) / RMS(h_prev)

        WHY NOT the absolute RMS of the state: the absolute RMS grows with
        the width of the model and with the weight scale, so a fixed
        membership center saturates for wide models (the membership hits
        1.0 at the first step and the recurrence degenerates to a single
        step) and never fires for narrow ones. The relative step size is
        dimensionless, so the same defaults work for every width.

        Polarity: 1.0 means "the recurrent step no longer changes the state"
        (the recurrence converged, more steps would not help); values close
        to 0 mean "the step still moves the state a lot".
        """
        prev_rms = h_prev.float().pow(2).mean(dim=(-1, -2)).sqrt()
        step_rms = (h_new - h_prev).float().pow(2).mean(dim=(-1, -2)).sqrt()
        rel = step_rms / (prev_rms + 1e-6)
        return 1.0 / (1.0 + rel)

    def _run_blocks(self, h: torch.Tensor) -> torch.Tensor:
        for t in range(self.cfg.num_shared_blocks):
            h = self.block(h + self.step_vector(t))
        return h

    # -- forward ------------------------------------------------------------

    def forward(self, input_ids: torch.Tensor) -> Tuple[torch.Tensor, int]:
        bsz = input_ids.shape[0]
        h = self.embed_tokens(input_ids)
        halted = torch.zeros(bsz, dtype=torch.bool, device=input_ids.device)
        steps_per_sample = torch.zeros(bsz, dtype=torch.int64, device=input_ids.device)
        steps = 0
        for t in range(self.cfg.num_shared_blocks):
            h_in = h + self.step_vector(t)
            h_new = self.block(h_in)
            if self.cfg.use_act_halting:
                running = ~halted
                # The confidence is a DECISION input: it is detached so the
                # membership gradient does not leak back into the block.
                conf = self.halt_confidence(h_new, h).detach()
                mu = self.halt_membership(conf)
                newly = (mu >= self.act_threshold) & running
                # WHY the output is ALWAYS applied: the step that triggers the
                # halt is a real recurrent step. If its output were dropped,
                # the shared block would stop influencing the logits and
                # would receive NO gradient (the trainer measured
                # grad_block == 0). The halting decision only decides
                # whether ANOTHER step is executed.
                #
                # The straight-through term (mu - mu.detach()) is exactly 0.0
                # in the forward pass, so the state is bit exact h_new, but it
                # carries d(mu)/d(center, width) in the backward pass: the
                # hard comparison is boolean, so this surrogate is the only
                # way for the fuzzy halting parameters to be learnable. The
                # delta is detached so the surrogate does not pollute the
                # block's own gradient.
                ste = (mu - mu.detach()).view(-1, 1, 1)
                h = torch.where(
                    running.view(-1, 1, 1),
                    h_new + ste * (h_new - h).detach(),
                    h,
                )
                halted = halted | newly
                steps_per_sample = steps_per_sample + running.to(torch.int64)
            else:
                h = h_new
                steps_per_sample = steps_per_sample + 1
            steps = t + 1
            if bool(halted.all()):
                break
        self.last_steps_per_sample = steps_per_sample
        logits = self.lm_head(self.norm(h))
        return logits, steps

    # -- latent (thought) path ---------------------------------------------

    def encode_latents(self, input_ids: torch.Tensor, n_latents: int) -> torch.Tensor:
        """Compress the token sequence into n_latents latent vectors.

        The recurrent stack runs over the tokens (no halting: the latent
        path is the "thinking" phase) and the resulting hidden states are
        pooled with contiguous segment means. Returns [B, n, hidden] with
        n = min(n_latents, T) (at least 1).
        """
        if n_latents < 1:
            raise ValueError("n_latents must be >= 1")
        h = self.embed_tokens(input_ids)
        h = self._run_blocks(h)
        bsz, seq, hidden = h.shape
        n = max(1, min(int(n_latents), seq))
        bounds = torch.linspace(0, seq, n + 1).round().long().tolist()
        chunks = []
        for i in range(n):
            start, end = bounds[i], max(bounds[i] + 1, bounds[i + 1])
            chunks.append(h[:, start:end, :].mean(dim=1))
        return torch.stack(chunks, dim=1)

    def forward_latents(self, latents: torch.Tensor) -> torch.Tensor:
        """Run the recurrent stack over latent vectors [B, n, hidden].

        Returns the logits of the LAST latent: [B, vocab].
        """
        if latents.dim() != 3:
            raise ValueError("latents must have shape [B, n, hidden]")
        h = self._run_blocks(latents)
        return self.lm_head(self.norm(h[:, -1]))


# --------------------------------------------------------------------------
# Persistence + safetensors loading
# --------------------------------------------------------------------------


def save_bitfuzzy(model: BitFuzzyRecurrent, path: str) -> None:
    """Save a student (config + state dict) with torch.save.

    The LIVE halting threshold (model.act_threshold) is persisted instead of
    the config copy: a threshold tuned at runtime with set_act_threshold must
    survive the save / load round-trip.
    """
    config = asdict(model.cfg)
    config["act_threshold"] = float(model.act_threshold)
    blob = {
        "format": "bitfuzzy-v1",
        "config": config,
        "state_dict": model.state_dict(),
    }
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    torch.save(blob, path)


def load_bitfuzzy(path: str) -> BitFuzzyRecurrent:
    """Load a student saved by save_bitfuzzy."""
    blob = torch.load(path, map_location="cpu", weights_only=False)
    raw_cfg = dict(blob["config"])
    dense = DenseFalconConfig(**raw_cfg["dense"])
    student_kwargs = {k: v for k, v in raw_cfg.items() if k != "dense"}
    model = BitFuzzyRecurrent(StudentConfig(dense=dense, **student_kwargs))
    model.load_state_dict(blob["state_dict"])
    return model


def load_dense_from_safetensors(dir: str, cfg: DenseFalconConfig) -> DenseFalcon3:
    """Load a dense teacher from a local directory of *.safetensors files.

    Expected HuggingFace key names:
        model.embed_tokens.weight
        model.layers.N.input_layernorm.weight
        model.layers.N.self_attn.{q,k,v,o}_proj.weight
        model.layers.N.post_attention_layernorm.weight
        model.layers.N.mlp.{gate,up,down}_proj.weight
        model.norm.weight
        lm_head.weight            (optional: absent when tied)

    No network access and no transformers dependency: only the `safetensors`
    package is used. Checkpoints that store the 1.58 bit trits as uint8 (the
    HF BitNet packing) are unpacked into {-1, 0, +1} weights and scaled by
    the accompanying "*_weight_scale" when present; such a teacher is the
    QUANTIZED model, not the full precision one.
    """
    from safetensors.torch import load_file

    files = sorted(Path(dir).glob("*.safetensors"))
    if not files:
        raise FileNotFoundError(f"no *.safetensors found in {dir}")
    raw: Dict[str, torch.Tensor] = {}
    for path in files:
        raw.update(load_file(str(path)))

    model = DenseFalcon3(cfg)
    sd = model.state_dict()
    hidden = cfg.hidden_size
    q_dim = cfg.num_heads * cfg.head_dim
    k_dim = cfg.num_kv_heads * cfg.head_dim
    ffn = cfg.intermediate_size
    loaded: List[str] = []
    missing: List[str] = []

    def take(hf_key: str, target: str, shape: Tuple[int, ...]) -> None:
        if hf_key not in raw:
            missing.append(hf_key)
            return
        tensor = raw[hf_key]
        if tensor.dtype == torch.uint8:
            if len(shape) != 2:
                raise ValueError(f"cannot unpack a 1D uint8 tensor for {hf_key}")
            tensor = _unpack_hf_ternary(tensor, shape[0], shape[1])
            scale_key = hf_key[: -len(".weight")] + ".weight_scale"
            if scale_key in raw:
                tensor = tensor * float(raw[scale_key].reshape(-1)[0])
        if tuple(tensor.shape) != tuple(shape):
            raise ValueError(
                f"shape mismatch for {hf_key}: {tuple(tensor.shape)} != {tuple(shape)}"
            )
        sd[target].copy_(tensor.to(sd[target].dtype))
        loaded.append(hf_key)

    take("model.embed_tokens.weight", "embed_tokens.weight", (cfg.vocab_size, hidden))
    for i in range(cfg.num_layers):
        p = f"model.layers.{i}"
        take(f"{p}.input_layernorm.weight", f"layers.{i}.input_layernorm.weight", (hidden,))
        take(f"{p}.self_attn.q_proj.weight", f"layers.{i}.self_attn.q_proj.weight", (q_dim, hidden))
        take(f"{p}.self_attn.k_proj.weight", f"layers.{i}.self_attn.k_proj.weight", (k_dim, hidden))
        take(f"{p}.self_attn.v_proj.weight", f"layers.{i}.self_attn.v_proj.weight", (k_dim, hidden))
        take(f"{p}.self_attn.o_proj.weight", f"layers.{i}.self_attn.o_proj.weight", (hidden, q_dim))
        take(
            f"{p}.post_attention_layernorm.weight",
            f"layers.{i}.post_attention_layernorm.weight",
            (hidden,),
        )
        take(f"{p}.mlp.gate_proj.weight", f"layers.{i}.mlp.gate_proj.weight", (ffn, hidden))
        take(f"{p}.mlp.up_proj.weight", f"layers.{i}.mlp.up_proj.weight", (ffn, hidden))
        take(f"{p}.mlp.down_proj.weight", f"layers.{i}.mlp.down_proj.weight", (hidden, ffn))
    take("model.norm.weight", "norm.weight", (hidden,))
    if "lm_head.weight" in raw:
        take("lm_head.weight", "lm_head.weight", (cfg.vocab_size, hidden))
    elif cfg.tie_embeddings:
        missing.append("lm_head.weight (tied)")
    else:
        missing.append("lm_head.weight")
    model.load_state_dict(sd)
    model.load_report = {"loaded": loaded, "missing": missing, "files": [str(f) for f in files]}
    return model


# --------------------------------------------------------------------------
# Footprint
# --------------------------------------------------------------------------


def footprint_report(model: nn.Module) -> Dict[str, Any]:
    """Parameter/byte footprint of a model, split by tensor kind.

    Keys: total_params, trainable_params, ternary_params, dense_params,
    packed_bytes (2-bit packed trits), fp32_bytes, compression_ratio,
    modules (per module breakdown), n_modules.
    """
    total_params = 0
    trainable_params = 0
    ternary_params = 0
    dense_params = 0
    packed_bytes = 0
    fp32_bytes = 0
    modules: List[Dict[str, Any]] = []
    seen: set = set()
    for name, module in model.named_modules():
        for pname, param in module.named_parameters(recurse=False):
            if id(param) in seen:
                continue
            seen.add(id(param))
            count = int(param.numel())
            total_params += count
            if param.requires_grad:
                trainable_params += count
            is_ternary = isinstance(module, (BitLinear, SugenoBitLinear))
            if is_ternary:
                ternary_params += count
                packed_bytes += (count + 3) // 4
            else:
                dense_params += count
            fp32_bytes += count * 4
            modules.append(
                {
                    "name": f"{name}.{pname}" if name else pname,
                    "params": count,
                    "kind": "ternary" if is_ternary else "dense",
                    "shape": tuple(param.shape),
                }
            )
    ratio = (fp32_bytes / packed_bytes) if packed_bytes else 0.0
    return {
        "total_params": total_params,
        "trainable_params": trainable_params,
        "ternary_params": ternary_params,
        "dense_params": dense_params,
        "packed_bytes": int(packed_bytes),
        "fp32_bytes": int(fp32_bytes),
        "compression_ratio": float(ratio),
        "n_modules": len(modules),
        "modules": modules,
    }


# --------------------------------------------------------------------------
# Self test
# --------------------------------------------------------------------------


def _tiny_dense_cfg() -> DenseFalconConfig:
    return DenseFalconConfig(
        hidden_size=64,
        num_layers=2,
        num_heads=4,
        num_kv_heads=2,
        head_dim=16,
        intermediate_size=128,
        vocab_size=128,
    )


def _selftest() -> int:
    torch.manual_seed(1234)
    results: List[Tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        results.append((name, bool(ok), detail))

    cfg = _tiny_dense_cfg()
    ids = torch.randint(0, cfg.vocab_size, (2, 5))

    # 1. dense teacher forward
    try:
        dense = DenseFalcon3(cfg)
        logits = dense(ids)
        ok = tuple(logits.shape) == (2, 5, cfg.vocab_size) and bool(torch.isfinite(logits).all())
        check("dense_forward", ok, f"shape={tuple(logits.shape)}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("dense_forward", False, f"exception: {exc}")
        dense = None

    # 2. student forward
    student_cfg = StudentConfig(
        dense=cfg,
        num_shared_blocks=3,
        num_experts=4,
        top_k=2,
        step_embeddings=8,
        use_act_halting=True,
        lowrank_rank=0,
    )
    try:
        student = BitFuzzyRecurrent(student_cfg)
        s_logits, steps = student(ids)
        ok = (
            tuple(s_logits.shape) == (2, 5, cfg.vocab_size)
            and bool(torch.isfinite(s_logits).all())
            and 1 <= steps <= student_cfg.num_shared_blocks
        )
        check("student_forward", ok, f"shape={tuple(s_logits.shape)} steps_used={steps}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("student_forward", False, f"exception: {exc}")
        student = None

    # 3. backward: finite grads and a LIVE fuzzy STE path
    if student is not None:
        try:
            loss = s_logits.float().pow(2).mean()
            loss.backward()
            all_finite = True
            for p in student.parameters():
                if p.grad is not None and not bool(torch.isfinite(p.grad).all()):
                    all_finite = False
                    break
            ternary_live = False
            for module in student.modules():
                if isinstance(module, (BitLinear, SugenoBitLinear)):
                    grad = module.weight.grad
                    if grad is not None and bool((grad != 0).any()):
                        ternary_live = True
                        break
            check("student_backward_finite", all_finite, f"loss={float(loss.detach()):.6f}")
            check("fuzzy_ste_gradient_live", ternary_live, "a ternary weight got a non-zero grad")
        except Exception as exc:  # pragma: no cover - diagnostic path
            check("student_backward_finite", False, f"exception: {exc}")
            check("fuzzy_ste_gradient_live", False, f"exception: {exc}")

    # 4. pack/unpack roundtrip is exact
    try:
        src = torch.tensor([[1.0, -1.0, 0.0, 1.0], [0.0, -1.0, 1.0, 0.0]])
        packed = ternary_pack(src)
        back = ternary_unpack(packed, tuple(src.shape))
        ok = packed.dtype == torch.uint8 and bool(torch.equal(back.to(torch.int8), src.to(torch.int8)))
        check("ternary_pack_roundtrip", ok, f"packed_bytes={packed.numel()}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("ternary_pack_roundtrip", False, f"exception: {exc}")

    # 5. fuzzy_quantize emits only trits
    try:
        w = torch.randn(8, 16) * 0.3
        q = fuzzy_quantize(w)
        uniq = set(torch.unique(q).tolist())
        ok = uniq.issubset({-1.0, 0.0, 1.0})
        check("fuzzy_quantize_trits", ok, f"unique={sorted(uniq)}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("fuzzy_quantize_trits", False, f"exception: {exc}")

    # 6. footprint report
    try:
        rep = footprint_report(student if student is not None else dense)
        needed = {"total_params", "trainable_params", "ternary_params", "packed_bytes", "fp32_bytes"}
        ok = (
            needed.issubset(rep.keys())
            and rep["total_params"] > 0
            and rep["packed_bytes"] > 0
            and rep["packed_bytes"] <= rep["fp32_bytes"]
        )
        check(
            "footprint_report",
            ok,
            f"params={rep['total_params']} packed={rep['packed_bytes']}B fp32={rep['fp32_bytes']}B",
        )
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("footprint_report", False, f"exception: {exc}")

    # 7. fixed-point view of the router is buildable
    try:
        moe = student.block.mlp
        hidden_states = torch.randn(2, 5, cfg.hidden_size)
        fp = moe.quantize_routing_fixed_point(bits=8, x=hidden_states)
        ok = (
            fp["centers_q"].dtype == torch.int32
            and fp["scores_q"].dtype == torch.int32
            and int(fp["scores_q"].max()) <= fp["levels"]
        )
        check("routing_fixed_point", ok, f"levels={fp['levels']}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("routing_fixed_point", False, f"exception: {exc}")

    # 8. latent (thought) path
    try:
        latents = student.encode_latents(ids, 3)
        latent_logits = student.forward_latents(latents)
        ok = (
            tuple(latents.shape) == (2, 3, cfg.hidden_size)
            and tuple(latent_logits.shape) == (2, cfg.vocab_size)
            and bool(torch.isfinite(latent_logits).all())
        )
        check("latent_path", ok, f"latents={tuple(latents.shape)}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("latent_path", False, f"exception: {exc}")

    # 9. save / load roundtrip
    try:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            path = str(Path(tmp) / "student.pt")
            save_bitfuzzy(student, path)
            reloaded = load_bitfuzzy(path)
            reloaded.eval()
            with torch.no_grad():
                again, _ = reloaded(ids)
            ok = bool(torch.allclose(again, s_logits, atol=1e-5))
        check("save_load_roundtrip", ok, f"path={path}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("save_load_roundtrip", False, f"exception: {exc}")

    # 10. safetensors key mapping (HF names, local dir, no network)
    try:
        import tempfile

        from safetensors.torch import save_file

        hf_cfg = DenseFalconConfig(
            hidden_size=cfg.hidden_size,
            num_layers=cfg.num_layers,
            num_heads=cfg.num_heads,
            num_kv_heads=cfg.num_kv_heads,
            head_dim=cfg.head_dim,
            intermediate_size=cfg.intermediate_size,
            vocab_size=cfg.vocab_size,
            tie_embeddings=False,
        )
        q_dim = hf_cfg.num_heads * hf_cfg.head_dim
        k_dim = hf_cfg.num_kv_heads * hf_cfg.head_dim
        tensors: Dict[str, torch.Tensor] = {
            "model.embed_tokens.weight": torch.randn(hf_cfg.vocab_size, hf_cfg.hidden_size),
            "model.norm.weight": torch.randn(hf_cfg.hidden_size),
            "lm_head.weight": torch.randn(hf_cfg.vocab_size, hf_cfg.hidden_size),
        }
        for i in range(hf_cfg.num_layers):
            p = f"model.layers.{i}"
            tensors[f"{p}.input_layernorm.weight"] = torch.randn(hf_cfg.hidden_size)
            tensors[f"{p}.post_attention_layernorm.weight"] = torch.randn(hf_cfg.hidden_size)
            tensors[f"{p}.self_attn.q_proj.weight"] = torch.randn(q_dim, hf_cfg.hidden_size)
            tensors[f"{p}.self_attn.k_proj.weight"] = torch.randn(k_dim, hf_cfg.hidden_size)
            tensors[f"{p}.self_attn.v_proj.weight"] = torch.randn(k_dim, hf_cfg.hidden_size)
            tensors[f"{p}.self_attn.o_proj.weight"] = torch.randn(hf_cfg.hidden_size, q_dim)
            tensors[f"{p}.mlp.gate_proj.weight"] = torch.randn(
                hf_cfg.intermediate_size, hf_cfg.hidden_size
            )
            tensors[f"{p}.mlp.up_proj.weight"] = torch.randn(
                hf_cfg.intermediate_size, hf_cfg.hidden_size
            )
            tensors[f"{p}.mlp.down_proj.weight"] = torch.randn(
                hf_cfg.hidden_size, hf_cfg.intermediate_size
            )
        with tempfile.TemporaryDirectory() as tmp:
            save_file(tensors, str(Path(tmp) / "model.safetensors"))
            teacher = load_dense_from_safetensors(tmp, hf_cfg)
            out = teacher(ids)
        missing = list(getattr(teacher, "load_report", {}).get("missing", []))
        ok = not missing and tuple(out.shape) == (2, 5, hf_cfg.vocab_size)
        check("safetensors_load", ok, f"missing={missing}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("safetensors_load", False, f"exception: {exc}")

    # 11. the shared block really influences the logits: the output of the
    #     step that triggers the halt must NOT be discarded
    try:
        probe = BitFuzzyRecurrent(student_cfg)
        probe.eval()
        with torch.no_grad():
            base_logits, _ = probe(ids)
            w = probe.block.self_attn.o_proj.weight
            # Flip the trit of the largest entry of every row. The ternary
            # forward is hard (the trits) and the row absmean is untouched by
            # a sign flip, so this is a real change of the block's
            # arithmetic, not a scale artefact.
            idx = w.detach().abs().argmax(dim=1)
            rows = torch.arange(w.shape[0])
            w[rows, idx] = -w[rows, idx]
            after_logits, _ = probe(ids)
        delta = float((after_logits - base_logits).abs().max())
        scale = float(base_logits.abs().max())
        ok = delta > 1e-3 * scale
        check(
            "block_influences_logits",
            ok,
            f"max|dlogits|={delta:.3e} rel={delta / max(scale, 1e-12):.3e}",
        )
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("block_influences_logits", False, f"exception: {exc}")

    # 12. the gradient reaches the shared block even when the halting fires
    #     at the FIRST step (the old code dropped that step's output, so the
    #     block got no gradient at all)
    try:
        probe = BitFuzzyRecurrent(student_cfg)
        probe.train()
        probe.set_act_threshold(0.0)  # every sample halts at the first step
        early_logits, early_steps = probe(ids)
        early_loss = early_logits.float().pow(2).mean()
        early_loss.backward()
        grad = probe.block.self_attn.q_proj.weight.grad
        grad_sum = float(grad.abs().sum()) if grad is not None else 0.0
        ok = grad_sum > 0.0 and early_steps == 1
        check(
            "block_grad_with_early_halt",
            ok,
            f"grad_sum={grad_sum:.3e} steps_used={early_steps}",
        )
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("block_grad_with_early_halt", False, f"exception: {exc}")

    # 13. the recurrence is NOT degenerate: the default config runs more than
    #     one recurrent step, and the halt rate is REPORTED (not hidden)
    try:
        probe = BitFuzzyRecurrent(student_cfg)
        used: List[int] = []
        halted_samples = 0
        total_samples = 0
        for k in range(4):
            gen = torch.Generator().manual_seed(9000 + k)
            ids_k = torch.randint(0, cfg.vocab_size, (2, 5), generator=gen)
            with torch.no_grad():
                _, used_k = probe(ids_k)
            per_sample = probe.last_steps_per_sample
            used.append(int(used_k))
            halted_samples += int((per_sample.detach() < student_cfg.num_shared_blocks).sum())
            total_samples += int(per_sample.numel())
        mean_steps = sum(used) / float(len(used))
        halt_rate = halted_samples / float(max(1, total_samples))
        ok = mean_steps >= 2.0
        detail = (
            f"mean_steps={mean_steps:.2f} steps={used} "
            f"halt_rate={halt_rate:.2f} (K={student_cfg.num_shared_blocks})"
        )
        check("recurrence_depth_reachable", ok, detail)
        print("[selftest] halt report: %s" % detail)
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("recurrence_depth_reachable", False, f"exception: {exc}")

    # 14. a threshold tuned at runtime survives the save / load round-trip
    try:
        import tempfile

        probe = BitFuzzyRecurrent(student_cfg)
        probe.set_act_threshold(0.42)
        with tempfile.TemporaryDirectory() as tmp:
            path = str(Path(tmp) / "student_threshold.pt")
            save_bitfuzzy(probe, path)
            back = load_bitfuzzy(path)
        ok = abs(float(back.act_threshold) - 0.42) < 1e-9
        check("halt_threshold_roundtrip", ok, f"threshold={back.act_threshold}")
    except Exception as exc:  # pragma: no cover - diagnostic path
        check("halt_threshold_roundtrip", False, f"exception: {exc}")

    failures = 0
    for name, ok, detail in results:
        if not ok:
            failures += 1
        print(f"{'PASS' if ok else 'FAIL'}: {name}" + (f" ({detail})" if detail else ""))
    print(f"SELFTEST {'PASS' if failures == 0 else 'FAIL'}: {len(results) - failures}/{len(results)} checks")
    return 0 if failures == 0 else 1


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="BitFuzzy recurrent MoE student (host-side research code)")
    parser.add_argument("--selftest", action="store_true", help="run the built-in self test")
    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.selftest:
        return _selftest()
    parser.print_help()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

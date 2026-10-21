#!/usr/bin/env python3
# -*- coding: ascii -*-
"""
train_distill.py -- trainer for the "BitFuzzy recurrent MoE" student distilled
from a dense Falcon3 teacher.

The MODEL implementation lives in tools/bitfuzzy_moe_recurrent.py (parallel
lane).  This file owns ONLY the training pipeline:

  angular   Phase 1  ShortGPT-style angular analysis of the dense teacher.
  collapse  Phase 2  Depth recurrence: drop pruned layers, seed the shared block.
  distill   Phase 3-5  KL + CE + sparsity distillation (optional latent path).
  lowrank   Phase 6  SVD low-rank A x B on the expert projections + re-freeze.
  report    Phase 7  Footprint / compute budget (the <500MB decision number).

HONESTY RULES ENFORCED HERE (repo rule: a false claim is the worst defect):
  * A real (non-synthetic) run REQUIRES a CUDA GPU.  Without one we print an
    actionable error including the computed VRAM estimate and exit non-zero.
    There is NO silent CPU fallback for a real run.
  * --synthetic builds a TINY random teacher so the whole chain runs on CPU in
    a couple of minutes.  Every artifact produced that way is stamped
    "trained: false (synthetic)".
  * --dry-run prints the plan (phases, shapes, VRAM estimate, outputs) and
    exits without training.
  * A missing model dir / checkpoint is an actionable one-line ERROR, never a
    stack trace.
  * Numbers are MEASURED from real tensors (param counts, packed byte counts,
    hook-counted block calls), never estimated from formulas when the real
    thing can be counted.

Only torch + numpy + stdlib.  Comments are ASCII-only (PowerShell 5.1 on
Windows corrupts non-ASCII source files - documented repo rule).
"""

from __future__ import annotations

import argparse
import dataclasses
import inspect
import json
import math
import random
import sys
import time
from dataclasses import asdict, is_dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

BF: Any = None  # lazily imported model module (parallel lane)

TARGET_BYTES = 500 * 1000 * 1000  # the "<500MB" deployment target


class UserError(Exception):
    """Actionable failure: printed as one ERROR line, never as a traceback."""


# ---------------------------------------------------------------------------
# Lazy import of the model module
# ---------------------------------------------------------------------------

def bf_module() -> Any:
    """Import tools/bitfuzzy_moe_recurrent.py on demand."""
    global BF
    if BF is None:
        try:
            import bitfuzzy_moe_recurrent as m  # type: ignore
        except ImportError as e:
            raise UserError(
                "tools/bitfuzzy_moe_recurrent.py is missing or failed to import (%s). "
                "This trainer targets that module's public API (DenseFalconConfig, "
                "StudentConfig, DenseFalcon3, BitFuzzyRecurrent, BitFuzzyMoE, "
                "Falcon3Block, BitLinear, SugenoBitLinear, fuzzy_quantize, "
                "ternary_pack, ternary_unpack, save_bitfuzzy, load_bitfuzzy, "
                "load_dense_from_safetensors, footprint_report)." % e
            ) from None
        BF = m
    return BF


# ---------------------------------------------------------------------------
# Config plumbing.
#
# The model module is written by another lane, so the trainer resolves config
# field names by ALIAS instead of hard-coding them.  A cosmetic rename there
# must not break this file; a genuinely different field is reported as an
# actionable error listing the fields the module actually exposes.
# ---------------------------------------------------------------------------

ALIASES: Dict[str, Tuple[str, ...]] = {
    "vocab_size": ("vocab_size", "vocab", "n_vocab", "vocab_dim"),
    "hidden_size": ("hidden_size", "hidden", "d_model", "n_embd", "dim", "model_dim"),
    "num_layers": ("num_layers", "n_layers", "num_hidden_layers", "layers", "depth"),
    "num_heads": ("num_heads", "n_heads", "num_attention_heads", "heads"),
    "num_kv_heads": ("num_kv_heads", "n_kv_heads", "num_key_value_heads", "kv_heads"),
    "head_dim": ("head_dim", "head_size", "q_head_dim", "attention_head_dim"),
    "intermediate_size": (
        "intermediate_size", "intermediate", "n_ff", "ffn_size", "ffn_hidden",
        "hidden_intermediate",
    ),
    "max_seq_len": (
        "max_seq_len", "max_position_embeddings", "seq_len", "max_seq",
        "context_length", "max_length",
    ),
    # StudentConfig: K recurrent steps over ONE weight-shared block.
    "num_blocks": (
        "num_shared_blocks", "num_blocks", "n_blocks", "blocks",
        "n_shared_blocks", "shared_blocks", "recurrent_blocks",
        "n_recurrent_blocks", "depth_recurrence",
    ),
    "num_latents": (
        "num_latents", "n_latents", "latents", "num_thoughts", "n_thoughts",
        "num_latent_tokens",
    ),
    "num_experts": ("num_experts", "n_experts", "experts", "num_moe_experts"),
    "top_k": ("top_k", "k", "num_top_k", "router_top_k"),
    "step_embeddings": (
        "step_embeddings", "n_step_embeddings", "step_emb", "num_step_embeddings",
        "step_pool",
    ),
    "use_act_halting": ("use_act_halting", "act_halting", "use_act"),
    "lowrank_rank": ("lowrank_rank", "low_rank", "rank"),
    "rope_theta": ("rope_theta", "rope_base", "theta"),
    "rms_eps": ("rms_eps", "eps", "layer_norm_eps"),
    "tie_embeddings": ("tie_embeddings", "tie_word_embeddings", "tied"),
}

#: StudentConfig fields that hold a NESTED DenseFalconConfig.
NESTED_DENSE_FIELDS = ("dense", "dense_config", "dense_cfg", "teacher_config",
                        "base", "base_config")


def class_fields(cls: Any) -> Dict[str, bool]:
    """Map field name -> has_default, for dataclasses or plain __init__."""
    out: Dict[str, bool] = {}
    try:
        if is_dataclass(cls):
            for f in dataclasses.fields(cls):
                has_def = (f.default is not dataclasses.MISSING) or (
                    f.default_factory is not dataclasses.MISSING
                )
                out[f.name] = has_def
            return out
    except Exception:
        pass
    try:
        sig = inspect.signature(cls.__init__)
        for name, p in sig.parameters.items():
            if name == "self":
                continue
            out[name] = p.default is not inspect.Parameter.empty
    except Exception:
        pass
    return out


def cfg_get(obj: Any, canonical: str, default: Any = None) -> Any:
    """Read a canonical field from a config object (or plain dict) by alias."""
    if obj is None:
        return default
    if isinstance(obj, dict):
        for alias in ALIASES.get(canonical, (canonical,)):
            if alias in obj and obj[alias] is not None:
                return obj[alias]
        return default
    for alias in ALIASES.get(canonical, (canonical,)):
        if hasattr(obj, alias):
            v = getattr(obj, alias)
            if v is not None:
                return v
    return default


def model_config(model: Any) -> Any:
    """The model config: the module exposes it as `.cfg` (and `.config`)."""
    for attr in ("config", "cfg"):
        v = getattr(model, attr, None)
        if v is not None:
            return v
    return None


def instantiate_config(cls: Any, values: Dict[str, Any],
                       defaults: Optional[Dict[str, Any]] = None) -> Any:
    """Build a config object, mapping canonical names onto real field names."""
    fields = class_fields(cls)
    kwargs: Dict[str, Any] = {}
    for canonical, val in values.items():
        if val is None:
            continue
        for alias in ALIASES.get(canonical, (canonical,)):
            if alias in fields:
                kwargs[alias] = val
                break
    # Nested dense config (StudentConfig.dense).
    for name in fields:
        if name in NESTED_DENSE_FIELDS and "dense_config_obj" in values:
            kwargs[name] = values["dense_config_obj"]
            break
    if defaults:
        for canonical, val in defaults.items():
            if val is None:
                continue
            for alias in ALIASES.get(canonical, (canonical,)):
                if alias in fields and alias not in kwargs:
                    kwargs[alias] = val
                    break
    missing = sorted(n for n, has_def in fields.items()
                     if not has_def and n not in kwargs)
    if missing:
        raise UserError(
            "cannot build %s: required field(s) %s are not known to this trainer. "
            "Fields exposed by the module: %s"
            % (cls.__name__, missing, sorted(fields))
        )
    try:
        return cls(**kwargs)
    except TypeError as e:
        raise UserError(
            "cannot build %s: %s. Fields exposed by the module: %s"
            % (cls.__name__, e, sorted(fields))
        ) from None


def instantiate_model(cls: Any, cfg: Any) -> Any:
    """Construct a model from a config, tolerating the usual ctor shapes."""
    attempts = [lambda: cls(cfg), lambda: cls(config=cfg)]
    if is_dataclass(cfg):
        attempts.append(lambda: cls(**asdict(cfg)))
    last = "no attempt ran"
    for fn in attempts:
        try:
            return fn()
        except TypeError as e:
            last = str(e)
    raise UserError("cannot construct %s from the given config: %s" % (cls.__name__, last))


# ---------------------------------------------------------------------------
# Model-module API shims (exact signatures live in the parallel lane)
# ---------------------------------------------------------------------------

def load_dense_from_dir(path: Path, cfg: Any) -> Any:
    """load_dense_from_safetensors(dir, cfg) -> DenseFalcon3 (tolerant)."""
    fn = getattr(bf_module(), "load_dense_from_safetensors", None)
    if fn is None:
        raise UserError("bitfuzzy_moe_recurrent.load_dense_from_safetensors is missing")
    last = ""
    for args in ((str(path), cfg), (str(path),), ()):
        try:
            res = fn(*args) if args else fn()
        except TypeError as e:
            last = str(e)
            continue
        except FileNotFoundError as e:
            raise UserError("dense model not found in %s: %s" % (path, e)) from None
        except Exception as e:  # noqa: BLE001 - surfaced as an actionable error
            raise UserError("failed to load dense model from %s: %s" % (path, e)) from None
        return _unwrap_model(res)
    raise UserError("load_dense_from_safetensors has an unsupported signature (%s)" % last)


def _unwrap_model(res: Any) -> Any:
    """Accept model, or (model, cfg), or (cfg, model) tuples."""
    if isinstance(res, tuple) and len(res) == 2:
        a, b = res
        if hasattr(a, "forward"):
            return a
        if hasattr(b, "forward"):
            return b
    return res


def save_student(student: nn.Module, path: Path) -> Path:
    """save_bitfuzzy(model, path) with signature tolerance."""
    fn = getattr(bf_module(), "save_bitfuzzy", None)
    if fn is None:
        raise UserError("bitfuzzy_moe_recurrent.save_bitfuzzy is missing")
    path.parent.mkdir(parents=True, exist_ok=True)
    last = ""
    for call in (
        lambda: fn(student, str(path)),
        lambda: fn(str(path), student),
        lambda: fn(model=student, path=str(path)),
    ):
        try:
            call()
            return path
        except TypeError as e:
            last = str(e)
    raise UserError("save_bitfuzzy has an unsupported signature (%s)" % last)


def load_student(path: Path) -> nn.Module:
    """load_bitfuzzy(path) -> student module (signature tolerant)."""
    fn = getattr(bf_module(), "load_bitfuzzy", None)
    if fn is None:
        raise UserError("bitfuzzy_moe_recurrent.load_bitfuzzy is missing")
    last = ""
    for call in (lambda: fn(str(path)), lambda: fn(path=str(path))):
        try:
            res = call()
        except TypeError as e:
            last = str(e)
            continue
        except Exception as e:  # noqa: BLE001 - surfaced as an actionable error
            raise UserError("failed to load student checkpoint %s: %s" % (path, e)) from None
        if isinstance(res, nn.Module) or hasattr(res, "forward"):
            return res
        raise UserError("load_bitfuzzy(%s) did not return a module" % path)
    raise UserError("load_bitfuzzy has an unsupported signature (%s)" % last)


def fuzzy_quantize_(tensor: torch.Tensor) -> torch.Tensor:
    """fuzzy_quantize(t) -> ternary tensor (fuzzy STE), documented fallback."""
    fn = getattr(bf_module(), "fuzzy_quantize", None)
    if fn is not None:
        for call in (lambda: fn(tensor), lambda: fn(tensor, -1)):
            try:
                out = call()
                if isinstance(out, torch.Tensor):
                    return out
            except TypeError:
                continue
            except Exception:  # noqa: BLE001 - fall through to the local quantizer
                break
    # Local fallback: hard ternary {-1,0,1} (the kernel-side packing rule).
    sign = torch.sign(tensor)
    return torch.where(sign.abs() < 0.5, torch.zeros_like(sign), sign)


def ternary_bytes(tensor: torch.Tensor) -> Optional[int]:
    """Real byte count of a ternary tensor packed at 4 weights/byte."""
    fn = getattr(bf_module(), "ternary_pack", None)
    if fn is not None:
        try:
            packed = fn(tensor)
            if isinstance(packed, torch.Tensor):
                return int(packed.numel())
            if isinstance(packed, (bytes, bytearray)):
                return int(len(packed))
            n = len(packed)
            if n:
                return int(n)
        except Exception:  # noqa: BLE001 - fall back to the arithmetic count
            pass
    numel = int(tensor.numel())
    if numel == 0:
        return 0
    return (numel + 3) // 4


def module_footprint_report(model: nn.Module) -> Any:
    fn = getattr(bf_module(), "footprint_report", None)
    if fn is None:
        return None
    try:
        return fn(model)
    except Exception:  # noqa: BLE001 - informational only, never fatal
        return None


# ---------------------------------------------------------------------------
# Small utilities
# ---------------------------------------------------------------------------

def set_seed(seed: int) -> None:
    random.seed(seed)
    np.random.seed(seed % (2 ** 32))
    torch.manual_seed(seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(seed)
        torch.backends.cudnn.deterministic = True
        torch.backends.cudnn.benchmark = False


def write_json(path: Path, payload: Dict[str, Any]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False), encoding="utf-8")
    return path


def out_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def fmt_bytes(n: float) -> str:
    if n >= 1e9:
        return "%.2f GB" % (n / 1e9)
    if n >= 1e6:
        return "%.2f MB" % (n / 1e6)
    if n >= 1e3:
        return "%.2f KB" % (n / 1e3)
    return "%d B" % int(n)


def human_params(n: int) -> str:
    if n >= 1e9:
        return "%.3fG" % (n / 1e9)
    if n >= 1e6:
        return "%.2fM" % (n / 1e6)
    if n >= 1e3:
        return "%.1fK" % (n / 1e3)
    return str(n)


# ---------------------------------------------------------------------------
# Dense teacher resolution
# ---------------------------------------------------------------------------

SYNTH_DENSE: Dict[str, int] = {
    "vocab_size": 128,
    "hidden_size": 64,
    "num_layers": 2,
    "num_heads": 4,
    "num_kv_heads": 2,
    "head_dim": 16,
    "intermediate_size": 128,
}


def dense_params_from_hf(cfg: Dict[str, Any]) -> Dict[str, Any]:
    """Map a HuggingFace-style config.json onto our canonical dense params."""
    hidden = int(cfg.get("hidden_size", 0))
    heads = int(cfg.get("num_attention_heads", 0))
    kv = int(cfg.get("num_key_value_heads", heads) or heads)
    head_dim = int(cfg.get("head_dim", 0) or (hidden // heads if heads else 0))
    return {
        "vocab_size": int(cfg.get("vocab_size", 0)),
        "hidden_size": hidden,
        "num_layers": int(cfg.get("num_hidden_layers", 0)),
        "num_heads": heads,
        "num_kv_heads": kv,
        "head_dim": head_dim,
        "intermediate_size": int(cfg.get("intermediate_size", 0) or 4 * hidden),
        "rope_theta": float(cfg.get("rope_theta", 1000042.0)),
        "tie_embeddings": bool(cfg.get("tie_word_embeddings", True)),
    }


def build_dense_config(params: Dict[str, Any]) -> Any:
    m = bf_module()
    return instantiate_config(m.DenseFalconConfig, dict(params))


def resolve_teacher(args: argparse.Namespace, device: torch.device
                    ) -> Tuple[nn.Module, Any, Dict[str, Any], str]:
    """Return (model, config, params, source_label)."""
    if args.synthetic:
        m = bf_module()
        params = dict(SYNTH_DENSE)
        cfg = build_dense_config(params)
        model = instantiate_model(m.DenseFalcon3, cfg)
        model.eval()
        return model.to(device), cfg, params, "synthetic"
    if not args.dense_dir:
        raise UserError(
            "a real run needs --dense-dir <safetensors dir> (or --synthetic for the "
            "tiny CI teacher). Nothing was given."
        )
    d = Path(args.dense_dir)
    if not d.is_dir():
        raise UserError("dense model dir not found: %s" % d)
    cfg_path = d / "config.json"
    if cfg_path.is_file():
        raw = json.loads(cfg_path.read_text(encoding="utf-8"))
        params = dense_params_from_hf(raw)
    else:
        raise UserError(
            "config.json not found in %s (needed to build the teacher config)" % d
        )
    cfg = build_dense_config(params)
    model = load_dense_from_dir(d, cfg)
    if not isinstance(model, nn.Module):
        raise UserError("load_dense_from_safetensors(%s) did not return a module" % d)
    model.eval()
    mcfg = model_config(model)
    if mcfg is not None:
        params = {
            "vocab_size": cfg_get(mcfg, "vocab_size", params["vocab_size"]),
            "hidden_size": cfg_get(mcfg, "hidden_size", params["hidden_size"]),
            "num_layers": cfg_get(mcfg, "num_layers", params["num_layers"]),
            "num_heads": cfg_get(mcfg, "num_heads", params["num_heads"]),
            "num_kv_heads": cfg_get(mcfg, "num_kv_heads", params["num_kv_heads"]),
            "head_dim": cfg_get(mcfg, "head_dim", params["head_dim"]),
            "intermediate_size": cfg_get(mcfg, "intermediate_size",
                                         params["intermediate_size"]),
        }
    return model.to(device), cfg, params, "dense-dir:%s" % d


# ---------------------------------------------------------------------------
# Data: byte-level tokenizer fallback (no BPE dependency)
# ---------------------------------------------------------------------------

def read_token_ids(path: Path, vocab: int) -> List[int]:
    """Byte-level tokenization: id = byte % vocab.

    WHY bytes and not BPE: the trainer must run without a tokenizer artifact
    and without network access.  Byte ids are a strict subset of any real
    vocabulary (Falcon3 has 131072 slots), so the teacher sees valid ids.
    """
    raw = path.read_bytes()
    if not raw:
        raise UserError("data file is empty: %s" % path)
    vocab = max(1, int(vocab))
    return [b % vocab for b in raw]


def make_batches(args: argparse.Namespace, vocab: int, n_seq: int, seed: int
                 ) -> List[torch.Tensor]:
    """Build fixed-length token batches (validation set or training set)."""
    g = torch.Generator().manual_seed(seed)
    seq = int(args.seq_len)
    if args.synthetic or not args.data:
        ids = torch.randint(0, max(1, vocab), (max(1, n_seq), seq), generator=g)
        return [ids]
    p = Path(args.data)
    if not p.is_file():
        raise UserError("data file not found: %s" % p)
    toks = read_token_ids(p, vocab)
    n = max(1, (len(toks) - 1) // seq)
    n = min(n, n_seq) if n_seq else n
    out: List[torch.Tensor] = []
    for i in range(n):
        chunk = toks[i * seq:(i - 1) * seq + seq + 1]
        if len(chunk) < seq:
            chunk = chunk + [0] * (seq - len(chunk))
        out.append(torch.tensor(chunk[:seq], dtype=torch.long).unsqueeze(0))
    if not out:
        raise UserError("data file %s is too short for seq_len=%d" % (p, seq))
    return out


def next_batch(batches: List[torch.Tensor], step: int, batch_size: int,
               device: torch.device) -> torch.Tensor:
    """Cycle through the prepared batches, batching them when possible."""
    if not batches:
        raise UserError("no training batches were prepared")
    if batch_size <= 1:
        return batches[step % len(batches)].to(device)
    rows = [batches[(step * batch_size + i) % len(batches)][0]
            for i in range(batch_size)]
    return torch.stack(rows).to(device)


# ---------------------------------------------------------------------------
# Module introspection helpers
# ---------------------------------------------------------------------------

def find_blocks(model: nn.Module) -> List[Tuple[str, nn.Module]]:
    """All transformer blocks, in definition order."""
    out: List[Tuple[str, nn.Module]] = []
    for name, m in model.named_modules():
        if "block" in type(m).__name__.lower() and m is not model:
            out.append((name, m))
    return out


def _tensor_from_output(out: Any) -> Optional[torch.Tensor]:
    if isinstance(out, torch.Tensor):
        return out if out.dim() == 3 else None
    if isinstance(out, (tuple, list)):
        for item in out:
            t = _tensor_from_output(item)
            if t is not None:
                return t
    return None


def capture_block_outputs(model: nn.Module, batches: List[torch.Tensor],
                          device: torch.device) -> List[List[torch.Tensor]]:
    """Run the model and collect each block's output hidden state."""
    blocks = find_blocks(model)
    if not blocks:
        raise UserError("no transformer blocks found in the model (cannot analyse layers)")
    store: List[Optional[torch.Tensor]] = [None] * len(blocks)
    handles = []

    def make_hook(idx: int):
        def hook(_mod, _args, out):
            store[idx] = _tensor_from_output(out)
        return hook

    for i, (_name, m) in enumerate(blocks):
        handles.append(m.register_forward_hook(make_hook(i)))
    per_layer: List[List[torch.Tensor]] = [[] for _ in blocks]
    try:
        with torch.no_grad():
            for b in batches:
                model(b.to(device))
                for i in range(len(blocks)):
                    if store[i] is not None:
                        per_layer[i].append(store[i].detach().to("cpu", torch.float32))
                for i in range(len(store)):
                    store[i] = None
    finally:
        for h in handles:
            h.remove()
    return per_layer


def angular_stats(per_layer: List[List[torch.Tensor]]) -> List[Dict[str, float]]:
    """Block Angular Distance d(h_l, h_{l+1}) = 1 - cos(h_l, h_{l+1}).

    Statistics are accumulated streaming (sum, sum of squares, count) so a long
    validation set does not need to be held in memory.
    """
    stats: List[Dict[str, float]] = []
    for l in range(len(per_layer) - 1):
        s = 0.0
        s2 = 0.0
        n = 0
        for a, b in zip(per_layer[l], per_layer[l + 1]):
            if a.shape[-1] != b.shape[-1]:
                continue
            x = a.reshape(-1, a.shape[-1])
            y = b.reshape(-1, b.shape[-1])
            cos = F.cosine_similarity(x, y, dim=-1, eps=1e-6)
            d = (1.0 - cos).to(torch.float64)
            s += float(d.sum())
            s2 += float((d * d).sum())
            n += int(d.numel())
        if n == 0:
            stats.append({"layer": l, "mean": float("nan"), "var": float("nan"),
                          "samples": 0})
            continue
        mean = s / n
        var = max(0.0, s2 / n - mean * mean)
        stats.append({"layer": l, "mean": mean, "var": var, "samples": n})
    return stats


def select_prune_candidates(stats: List[Dict[str, float]], ratio: float,
                             n_layers: int) -> List[int]:
    """Lowest-variance layers are the prune candidates (ShortGPT-style).

    The LAST layer has no successor, so it has no angular distance and cannot
    be a candidate - it is never dropped.
    """
    usable = [s for s in stats if s["samples"] > 0]
    if not usable:
        return []
    ratio = min(max(float(ratio), 0.0), 0.95)
    n_prune = int(round(ratio * n_layers))
    # Keep at least one layer alive; the last layer is never a candidate.
    n_prune = max(0, min(n_prune, len(usable), max(0, n_layers - 1)))
    ranked = sorted(usable, key=lambda s: (s["var"], s["layer"]))
    return sorted(int(s["layer"]) for s in ranked[:n_prune])


def evenly_spaced(items: List[int], k: int) -> List[int]:
    """k evenly spaced entries of items (deduped, order preserved)."""
    if k <= 0 or not items:
        return []
    if len(items) <= k:
        return list(items)
    idx = np.linspace(0, len(items) - 1, k)
    picked: List[int] = []
    for i in idx:
        j = int(round(float(i)))
        if 0 <= j < len(items) and items[j] not in picked:
            picked.append(items[j])
    return picked


# ---------------------------------------------------------------------------
# Student construction (Phase 2)
# ---------------------------------------------------------------------------

#: Dense block tensor -> student block tensor.  Explicit, not fuzzy: the
#: student's MLP is a MoE, so the dense SwiGLU seeds the FIRST expert and the
#: remaining experts keep their own initialisation (reported, never hidden).
BLOCK_TENSOR_MAP: Tuple[Tuple[str, str], ...] = (
    ("input_layernorm.weight", "input_layernorm.weight"),
    ("post_attention_layernorm.weight", "post_attention_layernorm.weight"),
    ("self_attn.q_proj.weight", "self_attn.q_proj.weight"),
    ("self_attn.k_proj.weight", "self_attn.k_proj.weight"),
    ("self_attn.v_proj.weight", "self_attn.v_proj.weight"),
    ("self_attn.o_proj.weight", "self_attn.o_proj.weight"),
    ("mlp.gate_proj.weight", "mlp.experts.0.gate.weight"),
    ("mlp.up_proj.weight", "mlp.experts.0.up.weight"),
    ("mlp.down_proj.weight", "mlp.experts.0.down.weight"),
)


def build_student_config(dense_cfg: Any, blocks: int, latents: int, seq_len: int,
                         experts: int, top_k: int, step_embeddings: int,
                         act_halting: bool) -> Any:
    m = bf_module()
    values = {
        "dense_config_obj": dense_cfg,
        "num_blocks": int(blocks),
        "num_latents": int(latents),
        "num_experts": int(experts),
        "top_k": int(top_k),
        "step_embeddings": int(step_embeddings),
        "use_act_halting": bool(act_halting),
    }
    defaults = {
        "num_experts": int(experts),
        "top_k": int(top_k),
        "step_embeddings": int(step_embeddings),
        "use_act_halting": bool(act_halting),
        "lowrank_rank": 0,
    }
    return instantiate_config(m.StudentConfig, values, defaults)


def build_student(dense_cfg: Any, blocks: int, latents: int, seq_len: int,
                  experts: int, top_k: int, step_embeddings: int,
                  act_halting: bool) -> Tuple[nn.Module, Any]:
    m = bf_module()
    scfg = build_student_config(dense_cfg, blocks, latents, seq_len, experts,
                                top_k, step_embeddings, act_halting)
    student = instantiate_model(m.BitFuzzyRecurrent, scfg)
    return student, scfg


def copy_block_weights(src: nn.Module, dst: nn.Module) -> Dict[str, Any]:
    """Copy matching tensors from a dense block into the student block.

    Weights are COPIED, never averaged: the student blocks are ternary, and
    the average of two ternary matrices is a dense non-ternary matrix that
    destroys the 2-bit property.  Copying keeps the weight distribution the
    teacher already learned.
    """
    src_params = dict(src.named_parameters())
    dst_params = dict(dst.named_parameters())
    copied: List[str] = []
    skipped: List[Dict[str, str]] = []
    pairs = list(BLOCK_TENSOR_MAP)
    for tname, tp in dst_params.items():
        # 1) explicit dense->student mapping (handles the MoE MLP).
        match = None
        for sname, dname in pairs:
            if dname == tname:
                match = src_params.get(sname)
                break
        # 2) identical relative path.
        if match is None:
            match = src_params.get(tname)
        # 3) unique suffix match.
        if match is None:
            cands = [sn for sn in src_params
                     if sn.endswith(tname) or tname.endswith(sn)]
            if len(cands) == 1:
                match = src_params[cands[0]]
        if match is None:
            skipped.append({"tensor": tname, "reason": "no source tensor"})
            continue
        if tuple(match.shape) != tuple(tp.shape):
            skipped.append({"tensor": tname,
                            "reason": "shape %s != %s" % (tuple(match.shape),
                                                          tuple(tp.shape))})
            continue
        with torch.no_grad():
            tp.copy_(match.detach().to(dtype=tp.dtype))
        copied.append(tname)
    return {"copied": copied, "skipped": skipped}


def copy_embeddings(src: nn.Module, dst: nn.Module) -> Dict[str, Any]:
    """Copy the token embedding when the shapes match."""
    src_params = dict(src.named_parameters())
    dst_params = dict(dst.named_parameters())

    def pick(params: Dict[str, torch.Tensor], names: Sequence[str]
             ) -> Optional[Tuple[str, torch.Tensor]]:
        for n in names:
            if n in params:
                return n, params[n]
        for n, p in params.items():
            low = n.lower()
            if any(k in low for k in names):
                return n, p
        return None

    names = ("embed_tokens.weight", "tok_embeddings.weight", "embed.weight",
             "wte.weight", "embeddings.weight")
    s = pick(src_params, names)
    d = pick(dst_params, names)
    if s is None or d is None:
        return {"copied": False, "reason": "embedding tensor not found"}
    if tuple(s[1].shape) != tuple(d[1].shape):
        return {"copied": False,
                "reason": "shape %s != %s" % (tuple(s[1].shape), tuple(d[1].shape))}
    with torch.no_grad():
        d[1].copy_(s[1].detach().to(dtype=d[1].dtype))
    return {"copied": True, "source": s[0], "target": d[0]}


def init_step_embedding(student: nn.Module) -> Dict[str, Any]:
    """Initialise the ternary step-embedding table.

    WHY ternary: the step embedding selects which recurrent step a token is
    decoded in.  A ternary table keeps the whole student in the 2-bit regime
    and gives each step a distinct +-1 code so the router can separate steps.
    """
    touched: List[Dict[str, Any]] = []
    for name, p in student.named_parameters():
        low = name.lower()
        if "step" not in low:
            continue
        if p.dim() != 2 or not p.is_floating_point():
            continue
        with torch.no_grad():
            p.copy_(fuzzy_quantize_(torch.randn_like(p)).to(dtype=p.dtype))
        touched.append({"tensor": name, "shape": list(p.shape)})
    return {"tables": touched, "count": len(touched)}


# ---------------------------------------------------------------------------
# Footprint / MACs (measured from real tensors, never from formulas)
# ---------------------------------------------------------------------------

COMPONENTS = ("embed", "attention", "experts", "step-emb", "other")
TERNARY_COMPONENTS = ("attention", "experts")
FP32_COMPONENTS = ("embed", "step-emb", "other")


def classify_tensor(name: str) -> str:
    low = name.lower()
    if "step" in low:
        return "step-emb"
    if any(k in low for k in ("embed", "wte", "lm_head", "unembed", "output_head")):
        return "embed"
    if any(k in low for k in ("expert", "moe", "sugeno", "gate_proj", "up_proj",
                              "down_proj", "router", "routing", "centers", "widths")):
        return "experts"
    if any(k in low for k in ("attn", "attention", "q_proj", "k_proj", "v_proj",
                              "o_proj", "self_attn")):
        return "attention"
    return "other"


def classify_buffer(name: str) -> str:
    low = name.lower()
    if "rope" in low or "cos" in low or "sin" in low or "inv_freq" in low:
        return "rope-cache"
    if "step" in low:
        return "step-emb"
    return "other"


def footprint(model: nn.Module) -> Dict[str, Any]:
    """Count real tensors: params, fp32 bytes, and 2-bit (4 weights/byte) bytes.

    Runtime caches (the RoPE cos/sin table) are counted SEPARATELY: they are
    real memory but they are not weights, and folding them into the weight
    total would inflate the deployment budget with a recomputable table.
    """
    comp: Dict[str, Dict[str, Any]] = {c: {"params": 0, "bytes_fp32": 0,
                                           "bytes_2bit": 0, "tensors": 0}
                                       for c in COMPONENTS}
    total_params = 0
    total_2bit = 0
    for name, p in model.named_parameters():
        c = classify_tensor(name)
        numel = int(p.numel())
        total_params += numel
        comp[c]["params"] += numel
        comp[c]["bytes_fp32"] += numel * 4
        comp[c]["tensors"] += 1
        if c in TERNARY_COMPONENTS:
            b = ternary_bytes(p.detach().to(torch.float32)) or 0
        else:
            # Embeddings and the step table are not ternary in this design.
            b = numel * 4
        comp[c]["bytes_2bit"] += b
        total_2bit += b
    buffer_bytes = 0
    buffer_detail: Dict[str, int] = {}
    for name, b in model.named_buffers():
        if not isinstance(b, torch.Tensor):
            continue
        n = int(b.numel()) * int(b.element_size())
        buffer_bytes += n
        key = classify_buffer(name)
        buffer_detail[key] = buffer_detail.get(key, 0) + n
    return {
        "total_params": total_params,
        "bytes_fp32": total_params * 4,
        "bytes_2bit_projected": total_2bit,
        "components": comp,
        "runtime_buffer_bytes": buffer_bytes,
        "runtime_buffers": buffer_detail,
        "packing_rule": {
            "ternary_components": list(TERNARY_COMPONENTS),
            "fp32_components": list(FP32_COMPONENTS),
            "note": "2-bit projection counts the real tensor elements packed 4/byte; "
                    "the fp32 column is the exact parameter storage cost. The dense "
                    "teacher has no MoE, so its SwiGLU MLP (gate/up/down) is "
                    "reported under 'experts' to keep both models comparable.",
        },
    }


def module_macs(m: nn.Module) -> int:
    """MACs of one call of a module, from its real tensors.

    A sparse MoE only evaluates `top_k` experts per token, so counting ALL
    expert matrices would overstate the cost by the expert count.
    """
    if hasattr(m, "experts") and hasattr(m, "top_k"):
        experts = list(m.experts)
        k = int(min(int(getattr(m, "top_k", 1)), max(1, len(experts))))
        per_expert = 0
        for e in experts:
            per_expert += sum(p.numel() for p in e.parameters() if p.dim() >= 2)
        routing = 0
        for nme, p in m.named_parameters(recurse=False):
            if "routing" in nme and p.dim() >= 2:
                routing += int(p.numel())
        return k * per_expert + routing
    return int(sum(p.numel() for p in m.parameters() if p.dim() >= 2))


def count_macs(model: nn.Module, sample: torch.Tensor, device: torch.device
               ) -> Dict[str, Any]:
    """MACs/token measured by hooking every block and counting real calls.

    Measuring beats estimating here: a recurrent student applies its shared
    block once per recurrence step, so the call count is only knowable at
    runtime.  Block MACs = sum of the block's matrix weights (each applied
    once per call); the vocabulary head is a matmul and is counted per token.
    """
    blocks = find_blocks(model)
    calls: Dict[str, int] = {}
    macs_per_call: Dict[str, int] = {}
    handles = []

    def make_hook(name: str):
        def hook(_mod, _args, _out):
            calls[name] = calls.get(name, 0) + 1
        return hook

    for name, m in blocks:
        macs_per_call[name] = module_macs(m)
        handles.append(m.register_forward_hook(make_hook(name)))
    was_training = model.training
    model.eval()
    try:
        with torch.no_grad():
            out = model(sample.to(device))
        if isinstance(out, tuple):
            out = out[0]
    finally:
        for h in handles:
            h.remove()
        model.train(was_training)
    total = 0
    for name, c in calls.items():
        total += c * macs_per_call.get(name, 0)
    head = 0
    # The vocabulary head is looked up as a MODULE, not as a parameter: with
    # tied embeddings `lm_head.weight` IS `embed_tokens.weight`, and
    # named_parameters() de-duplicates it, so a parameter-name lookup would
    # silently count ZERO head MACs (the dominant term on a real Falcon3).
    for name, m in model.named_modules():
        low = name.lower()
        if not any(k in low for k in ("lm_head", "unembed", "output_head",
                                      "embed_out")):
            continue
        for p in m.parameters(recurse=False):
            if p.dim() >= 2:
                head = max(head, int(p.numel()))
    tokens = int(sample.numel())
    macs_per_token = (total + head * tokens) / max(1, tokens)
    return {
        "macs_per_token": macs_per_token,
        "block_calls": calls,
        "blocks_measured": len(blocks),
        "vocab_head_macs_per_token": head,
        "tokens_measured": tokens,
    }


# ---------------------------------------------------------------------------
# VRAM estimate
# ---------------------------------------------------------------------------

def vram_breakdown(cfg: Any, batch_size: int = 1, seq_len: int = 128,
                   student_blocks: int = 0) -> Dict[str, Any]:
    """Estimate the VRAM a real run needs.

    teacher bf16 weights + student fp32 weights + AdamW states + activations.
    """
    vocab = int(cfg_get(cfg, "vocab_size", 0) or 0)
    hidden = int(cfg_get(cfg, "hidden_size", 0) or 0)
    layers = int(cfg_get(cfg, "num_layers", 0) or 0)
    inter = int(cfg_get(cfg, "intermediate_size", 0) or 0)
    heads = int(cfg_get(cfg, "num_heads", 0) or 0)
    kv = int(cfg_get(cfg, "num_kv_heads", heads) or heads)
    hd = int(cfg_get(cfg, "head_dim", 0) or (hidden // heads if heads else 0))
    kv_dim = kv * hd
    embed = 2 * vocab * hidden
    attn_per_layer = hidden * hidden * 2 + 2 * kv_dim * hidden + hidden * hidden
    mlp_per_layer = 3 * hidden * inter
    teacher_params = embed + layers * (attn_per_layer + mlp_per_layer)
    teacher_bytes = teacher_params * 2  # bf16
    blocks = student_blocks if student_blocks > 0 else layers
    student_params = embed + blocks * (attn_per_layer + mlp_per_layer)
    student_bytes = student_params * 4
    adam_bytes = student_params * 8  # 2 moments in fp32
    act_bytes = batch_size * seq_len * hidden * max(1, layers) * 2 * 4
    total = teacher_bytes + student_bytes + adam_bytes + act_bytes
    return {
        "teacher_params": teacher_params,
        "teacher_bf16_bytes": teacher_bytes,
        "student_params": student_params,
        "student_fp32_bytes": student_bytes,
        "adam_states_bytes": adam_bytes,
        "activations_bytes": act_bytes,
        "total_bytes": total,
        "total_gb": total / 1e9,
        "assumptions": {
            "teacher_dtype": "bf16 (2 bytes/param)",
            "student_dtype": "fp32 weights + fp32 Adam moments",
            "activations": "batch x seq x hidden x layers x 2 bytes x 4 (fp16 + temporaries)",
        },
    }


def estimate_vram_gb(cfg: Any, batch_size: int = 1, seq_len: int = 128,
                     student_blocks: int = 0) -> float:
    """VRAM estimate in GB (teacher + student + optimizer + activations)."""
    return vram_breakdown(cfg, batch_size, seq_len, student_blocks)["total_gb"]


# ---------------------------------------------------------------------------
# CUDA gate (honesty rule: a real run never silently falls back to CPU)
# ---------------------------------------------------------------------------

def require_cuda(args: argparse.Namespace, cfg: Any, student_blocks: int) -> None:
    if args.synthetic:
        return
    if torch.cuda.is_available():
        return
    est = vram_breakdown(cfg, args.batch_size, args.seq_len, student_blocks)
    print("ERROR: a real (non-synthetic) run requires a CUDA GPU.", file=sys.stderr)
    print("       torch.cuda.is_available() = False on this host.", file=sys.stderr)
    print("       estimate_vram_gb(cfg) = %.2f GB" % est["total_gb"], file=sys.stderr)
    print("       breakdown: teacher bf16 %s | student fp32 %s | AdamW %s | "
          "activations %s"
          % (fmt_bytes(est["teacher_bf16_bytes"]),
             fmt_bytes(est["student_fp32_bytes"]),
             fmt_bytes(est["adam_states_bytes"]),
             fmt_bytes(est["activations_bytes"])),
          file=sys.stderr)
    print("       Use --synthetic for the tiny CPU CI chain, or run on a CUDA host "
          "with >= %.2f GB free." % est["total_gb"], file=sys.stderr)
    raise SystemExit(2)


def preflight_teacher(args: argparse.Namespace) -> None:
    """Validate the teacher dir and the CUDA gate BEFORE loading any weight.

    The gate runs first on purpose: a real run on a CPU-only host must fail
    in seconds with the VRAM estimate, not after loading a 3B teacher.
    """
    if args.synthetic:
        return
    if not args.dense_dir:
        raise UserError(
            "a real run needs --dense-dir <safetensors dir> (or --synthetic for the "
            "tiny CI teacher). Nothing was given."
        )
    d = Path(args.dense_dir)
    if not d.is_dir():
        raise UserError("dense model dir not found: %s" % d)
    cfg_path = d / "config.json"
    if not cfg_path.is_file():
        raise UserError(
            "config.json not found in %s (needed to build the teacher config and to "
            "estimate the VRAM before loading the weights)" % d
        )
    try:
        raw = json.loads(cfg_path.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        raise UserError("could not parse %s: %s" % (cfg_path, e)) from None
    cfg = build_dense_config(dense_params_from_hf(raw))
    require_cuda(args, cfg, student_blocks=int(args.blocks))


# ---------------------------------------------------------------------------
# Phase 1: angular
# ---------------------------------------------------------------------------

def cmd_angular(args: argparse.Namespace) -> int:
    preflight_teacher(args)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    teacher, tcfg, params, source = resolve_teacher(args, device)
    require_cuda(args, tcfg, student_blocks=0)
    vocab = int(cfg_get(tcfg, "vocab_size", 0) or 0)
    if vocab <= 0:
        raise UserError("teacher config exposes no vocab_size")
    batches = make_batches(args, vocab, args.val_samples, args.seed)
    n_layers = len(find_blocks(teacher))
    if n_layers < 2:
        raise UserError("teacher has %d block(s): angular analysis needs >= 2" % n_layers)
    print("[angular] teacher=%s layers=%d val_batches=%d vocab=%d"
          % (source, n_layers, len(batches), vocab))
    per_layer = capture_block_outputs(teacher, batches, device)
    stats = angular_stats(per_layer)
    candidates = select_prune_candidates(stats, args.prune_ratio, n_layers)
    survivors = [l for l in range(n_layers) if l not in candidates]
    report = {
        "phase": "angular",
        "trained": False,
        "source": source,
        "synthetic": bool(args.synthetic),
        "model": {
            "layers": n_layers,
            "hidden_size": params.get("hidden_size"),
            "vocab_size": vocab,
            "heads": params.get("num_heads"),
            "kv_heads": params.get("num_kv_heads"),
            "intermediate_size": params.get("intermediate_size"),
        },
        "metric": "d(h_l,h_l+1) = 1 - cos(h_l, h_l+1) over all token positions",
        "prune_ratio": float(args.prune_ratio),
        "layers_stats": stats,
        "prune_candidates": candidates,
        "survivors": survivors,
        "note": "lowest-variance layers are the prune candidates; the last layer has "
                "no successor and is never dropped.",
    }
    out = Path(args.out) if args.out else Path("angular_report.json")
    write_json(out, report)
    print("[angular] per-layer angular distance (1 - cos):")
    print("  layer   mean      variance   samples")
    for s in stats:
        print("  %5d  %8.5f  %10.6f  %8d"
              % (s["layer"], s["mean"], s["var"], s["samples"]))
    print("[angular] prune candidates (lowest variance): %s" % candidates)
    print("[angular] survivors: %s" % survivors)
    print("[angular] wrote %s" % out)
    return 0


# ---------------------------------------------------------------------------
# Phase 2: collapse (depth recurrence)
# ---------------------------------------------------------------------------

def load_or_compute_candidates(args: argparse.Namespace, teacher: nn.Module,
                               tcfg: Any, device: torch.device
                               ) -> Tuple[List[int], List[int], int, str]:
    """Return (candidates, survivors, n_layers, origin)."""
    n_layers = len(find_blocks(teacher))
    if args.angular_report:
        p = Path(args.angular_report)
        if not p.is_file():
            raise UserError("angular report not found: %s" % p)
        try:
            rep = json.loads(p.read_text(encoding="utf-8"))
        except Exception as e:  # noqa: BLE001
            raise UserError("could not parse angular report %s: %s" % (p, e)) from None
        cands = [int(x) for x in rep.get("prune_candidates", [])]
        origin = "angular-report:%s" % p
    else:
        vocab = int(cfg_get(tcfg, "vocab_size", 0) or 0)
        batches = make_batches(args, vocab, max(1, args.val_samples), args.seed)
        per_layer = capture_block_outputs(teacher, batches, device)
        stats = angular_stats(per_layer)
        cands = select_prune_candidates(stats, args.prune_ratio, n_layers)
        origin = "inline-angular"
    cands = [c for c in cands if 0 <= c < n_layers]
    survivors = [l for l in range(n_layers) if l not in cands]
    return cands, survivors, n_layers, origin


def cmd_collapse(args: argparse.Namespace) -> int:
    preflight_teacher(args)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    teacher, tcfg, params, source = resolve_teacher(args, device)
    require_cuda(args, tcfg, student_blocks=args.blocks)
    cands, survivors, n_layers, origin = load_or_compute_candidates(
        args, teacher, tcfg, device)
    dense_layers = find_blocks(teacher)
    if not dense_layers:
        raise UserError("teacher exposes no blocks")
    k_req = int(args.blocks)
    picked = evenly_spaced(survivors, k_req)
    k_eff = len(picked)
    if k_eff < k_req:
        print("[collapse] WARNING: asked for %d recurrent step(s) but only %d "
              "surviving layer(s) remain -> K=%d" % (k_req, len(survivors), k_eff))
    # The student model is K recurrent steps over ONE weight-shared block
    # (BitFuzzyRecurrent.cfg.num_shared_blocks = K).  There is therefore ONE
    # block to seed; the K evenly spaced survivors are the candidate sources
    # and the selected one is copied verbatim (never averaged).
    src_layer = picked[len(picked) // 2] if picked else None
    print("[collapse] prune candidates=%s survivors=%s K=%d (requested %d)"
          % (cands, survivors, k_eff, k_req))
    print("[collapse] candidate source layers (evenly spaced survivors): %s" % picked)
    print("[collapse] shared block seeded from dense layer %s" % src_layer)
    student, scfg = build_student(tcfg, k_eff, args.latents, args.seq_len,
                                  args.experts, args.topk_experts, k_eff,
                                  not args.no_act_halting)
    student.to(device)
    sblocks = find_blocks(student)
    if not sblocks:
        raise UserError("student exposes no blocks - cannot seed the shared block")
    copy_report: Dict[str, Any] = {}
    if src_layer is not None and src_layer < len(dense_layers):
        copy_report = copy_block_weights(dense_layers[src_layer][1], sblocks[0][1])
        print("[collapse] copied %d tensor(s), skipped %d"
              % (len(copy_report["copied"]), len(copy_report["skipped"])))
    emb = copy_embeddings(teacher, student)
    step = init_step_embedding(student)
    if step["count"] == 0:
        print("[collapse] WARNING: no step-embedding table found by name; leaving the "
              "model's own initialisation in place")
    else:
        print("[collapse] step embedding: %d table(s) initialised with ternary values"
              % step["count"])
    out = out_dir(Path(args.out))
    ckpt = save_student(student, out / "student.bitfuzzy")
    sidecar = {
        "phase": "collapse",
        "trained": False,
        "synthetic": bool(args.synthetic),
        "source": source,
        "teacher": params,
        "student_config": _cfg_to_dict(scfg),
        "prune_candidates": cands,
        "survivors": survivors,
        "candidate_source_layers": picked,
        "shared_block_source_layer": src_layer,
        "blocks_requested": k_req,
        "recurrent_steps_K": k_eff,
        "candidates_origin": origin,
        "weight_init": "COPY (one surviving dense layer into the weight-shared "
                       "block). Weights are never averaged: the average of two "
                       "ternary matrices is dense and would destroy the 2-bit "
                       "property.",
        "recurrence_note": "BitFuzzyRecurrent implements depth recurrence as K "
                           "steps over ONE weight-shared block "
                           "(num_shared_blocks=K), so only one source layer can be "
                           "copied into it.",
        "step_embedding": step,
        "embedding_copy": emb,
        "block_copy": copy_report,
        "checkpoint": str(ckpt),
    }
    write_json(out / "student_config.json", sidecar)
    print("[collapse] saved %s" % ckpt)
    print("[collapse] saved %s" % (out / "student_config.json"))
    return 0


def _cfg_to_dict(cfg: Any) -> Dict[str, Any]:
    if cfg is None:
        return {}
    if is_dataclass(cfg):
        try:
            return asdict(cfg)
        except Exception:  # noqa: BLE001
            pass
    if isinstance(cfg, dict):
        return dict(cfg)
    out: Dict[str, Any] = {}
    for name in dir(cfg):
        if name.startswith("_"):
            continue
        try:
            v = getattr(cfg, name)
        except Exception:  # noqa: BLE001
            continue
        if isinstance(v, (int, float, str, bool)) or v is None:
            out[name] = v
    return out


# ---------------------------------------------------------------------------
# Phase 3-5: distill
# ---------------------------------------------------------------------------

def install_router_capture(student: nn.Module, sink: List[Tuple[Any, torch.Tensor]]
                           ) -> List[Any]:
    """Capture the MoE router INPUT so the routing distribution is real.

    The fuzzy router is not an nn.Module (it is parameters + math), so we
    capture the MoE input and recompute the router scores with the module's
    own `scores()` API after the forward - the routing distribution is then
    the model's real distribution, not a proxy.
    """
    handles = []

    def make_pre_hook(mod: Any):
        def hook(_mod, args):
            if args and isinstance(args[0], torch.Tensor):
                sink.append((mod, args[0]))
        return hook

    for _name, m in student.named_modules():
        if hasattr(m, "scores") or hasattr(m, "route_info"):
            handles.append(m.register_forward_pre_hook(make_pre_hook(m)))
    return handles


def routing_distribution(captured: List[Tuple[Any, torch.Tensor]]
                         ) -> Optional[torch.Tensor]:
    """Routing probability distribution [N, E] from the captured router input."""
    for mod, x in captured:
        if not hasattr(mod, "scores"):
            continue
        flat = x.reshape(-1, x.shape[-1])
        try:
            sc = mod.scores(flat)          # [E, N]
            return torch.softmax(sc, dim=0).transpose(0, 1)
        except Exception:  # noqa: BLE001 - the penalty is optional, never fatal
            continue
    return None


def sparsity_penalty(probs: Optional[torch.Tensor]) -> torch.Tensor:
    """Penalise routing entropy / over-spread.

    A uniform router spreads every token over every expert: maximum entropy and
    no specialisation.  The penalty is the mean NORMALISED entropy of the
    routing distribution, so it is 0 for a hard one-hot choice and 1 for a
    uniform spread.  Minimising it makes the routing decisive.
    """
    if probs is None:
        return torch.zeros((), dtype=torch.float32)
    p = probs
    if p.dim() < 2:
        p = p.reshape(1, -1)
    p = p.float().clamp_min(1e-9)
    ent = -(p * p.log()).sum(dim=-1)
    n_experts = int(p.shape[-1])
    if n_experts > 1:
        ent = ent / math.log(float(n_experts))
    return ent.mean()


def kl_topk_loss(student_logits: torch.Tensor, teacher_logits: torch.Tensor,
                 topk: int, temperature: float) -> torch.Tensor:
    """KL(student || teacher) restricted to the teacher's top-k logits.

    WHY temperature: the teacher distribution is far sharper than the student's
    early in training, and the raw KL is dominated by the teacher's argmax.
    Dividing both by T softens the teacher; the T^2 factor restores the
    gradient scale so the loss magnitude does not depend on T.
    """
    k = int(min(topk, teacher_logits.shape[-1]))
    t_top, idx = torch.topk(teacher_logits, k, dim=-1)
    t_logp = F.log_softmax(t_top / temperature, dim=-1)
    s_logp = F.log_softmax(student_logits.gather(-1, idx) / temperature, dim=-1)
    kl = (t_logp.exp() * (t_logp - s_logp)).sum(dim=-1)
    return kl.mean() * (temperature * temperature)


def cmd_distill(args: argparse.Namespace) -> int:
    preflight_teacher(args)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    teacher, tcfg, params, source = resolve_teacher(args, device)
    student, scfg, s_origin = prepare_student_for_training(args, teacher, tcfg, device)
    require_cuda(args, tcfg, student_blocks=len(find_blocks(student)))
    teacher.eval()
    for p in teacher.parameters():
        p.requires_grad_(False)
    student.train()
    vocab = int(cfg_get(tcfg, "vocab_size", 0) or 0)
    batches = make_batches(args, vocab, max(1, args.batch_size * 4), args.seed)
    out = out_dir(Path(args.out))
    ckpt_path = out / "student.bitfuzzy"
    log_path = Path(args.log) if args.log else out / "metrics.jsonl"
    log_path.parent.mkdir(parents=True, exist_ok=True)

    params_list = [p for p in student.parameters() if p.requires_grad]
    if not params_list:
        raise UserError("student has no trainable parameters")
    opt = torch.optim.AdamW(params_list, lr=float(args.lr), weight_decay=float(args.wd))
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=max(1, int(args.steps)))
    captured: List[Tuple[Any, torch.Tensor]] = []
    handles = install_router_capture(student, captured)
    trained_marker = ("trained: false (synthetic)" if args.synthetic
                      else "trained: true")
    print("[distill] teacher=%s student=%s params=%s"
          % (source, s_origin, human_params(sum(p.numel() for p in params_list))))
    print("[distill] steps=%d batch=%d lr=%g alpha=%g beta=%g gamma=%g topk=%d T=%g"
          % (args.steps, args.batch_size, args.lr, args.alpha, args.beta,
             args.gamma, args.topk, args.temperature))
    print("[distill] latents=%d (%s)" % (args.latents,
                                         "enabled" if args.latents > 0 else "disabled"))
    t0 = time.time()
    last_metrics: Dict[str, Any] = {}
    try:
        for step in range(int(args.steps)):
            captured.clear()
            ids = next_batch(batches, step, int(args.batch_size), device)
            with torch.no_grad():
                t_out = teacher(ids)
                t_logits = t_out[0] if isinstance(t_out, tuple) else t_out
            s_out = student(ids)
            s_logits, steps_used = (s_out if isinstance(s_out, tuple) else (s_out, 0))
            halted = 0
            per_sample = getattr(student, "last_steps_per_sample", None)
            if isinstance(per_sample, torch.Tensor) and per_sample.numel() > 0:
                halted = int((per_sample.detach() > 1).sum())
            loss_kl = kl_topk_loss(s_logits, t_logits, args.topk, args.temperature)
            tgt = ids[:, 1:].reshape(-1)
            loss_ce = F.cross_entropy(s_logits[:, :-1].reshape(-1, s_logits.shape[-1]), tgt)
            probs = routing_distribution(captured)
            loss_sp = sparsity_penalty(probs)
            loss = args.alpha * loss_kl + args.beta * loss_ce + args.gamma * loss_sp
            loss_lat = torch.zeros((), dtype=torch.float32, device=loss.device)
            if args.latents > 0 and hasattr(student, "encode_latents"):
                lat = student.encode_latents(ids, int(args.latents))
                l_out = student.forward_latents(lat)
                l_logits = l_out[0] if isinstance(l_out, tuple) else l_out
                # The latent path is a COMPRESSED summary of the sequence, so it
                # is aligned with the tail of the sequence it summarises.
                if l_logits.dim() == 2:
                    # [B, vocab]: the module returns the logits of the LAST latent.
                    loss_lat = F.cross_entropy(l_logits, ids[:, -1])
                elif l_logits.dim() == 3:
                    n_lat = int(l_logits.shape[1])
                    l_tgt = ids[:, -n_lat:].reshape(-1)
                    loss_lat = F.cross_entropy(
                        l_logits.reshape(-1, l_logits.shape[-1]), l_tgt)
                loss = loss + args.latent_weight * loss_lat
            opt.zero_grad(set_to_none=True)
            loss.backward()
            grad_norm = float(nn.utils.clip_grad_norm_(params_list, float(args.clip)))
            # Gradient coverage: the ACT halting DISCARDS the output of the
            # step where a sample halts, so the block can end up with NO
            # gradient at all (the loss would then train only the embedding
            # and the head).  We measure it instead of assuming it.
            grad_block = 0.0
            grad_rest = 0.0
            for pname, pp in student.named_parameters():
                if pp.grad is None:
                    continue
                g = float(pp.grad.abs().sum())
                if pname.startswith("block"):
                    grad_block += g
                else:
                    grad_rest += g
            opt.step()
            sched.step()
            metrics = {
                "step": step,
                "loss": float(loss.detach()),
                "kl": float(loss_kl.detach()),
                "ce": float(loss_ce.detach()),
                "sparsity": float(loss_sp.detach()),
                "latent_ce": float(loss_lat.detach()),
                "grad_norm": grad_norm,
                "grad_block": grad_block,
                "grad_other": grad_rest,
                "lr": float(sched.get_last_lr()[0]),
                "steps_used": int(steps_used),
                "halted_samples": halted,
                "tokens": int(ids.numel()),
                "trained": "false (synthetic)" if args.synthetic else "true",
            }
            if step == 0 and grad_block <= 0.0 and grad_rest > 0.0:
                print("[distill] WARNING: the shared block received NO gradient "
                      "(the ACT halting discarded the halted step, so the block "
                      "output never reaches the loss). The distillation would "
                      "train only the embedding/head. Re-run with "
                      "--no-act-halting to train the block.", file=sys.stderr)
            last_metrics = metrics
            with log_path.open("a", encoding="utf-8") as fh:
                fh.write(json.dumps(metrics) + "\n")
            print("[distill] step %d/%d loss=%.4f kl=%.4f ce=%.4f sparse=%.4f "
                  "lat_ce=%.4f lr=%.3g steps_used=%s"
                  % (step, args.steps, metrics["loss"], metrics["kl"], metrics["ce"],
                     metrics["sparsity"], metrics["latent_ce"], metrics["lr"],
                     metrics["steps_used"]))
            if args.save_every > 0 and (step + 1) % int(args.save_every) == 0:
                p = save_student(student, out / ("student_step%d.bitfuzzy" % (step + 1)))
                print("[distill] checkpoint -> %s" % p)
    finally:
        for h in handles:
            h.remove()
    final = save_student(student, ckpt_path)
    summary = {
        "phase": "distill",
        "trained": not bool(args.synthetic),
        "trained_marker": trained_marker,
        "synthetic": bool(args.synthetic),
        "source": source,
        "student_origin": s_origin,
        "steps": int(args.steps),
        "batch_size": int(args.batch_size),
        "lr": float(args.lr),
        "alpha": float(args.alpha),
        "beta": float(args.beta),
        "gamma": float(args.gamma),
        "latents": int(args.latents),
        "latent_weight": float(args.latent_weight),
        "topk": int(args.topk),
        "temperature": float(args.temperature),
        "clip": float(args.clip),
        "seed": int(args.seed),
        "seconds": round(time.time() - t0, 3),
        "last_metrics": last_metrics,
        "checkpoint": str(final),
        "metrics_log": str(log_path),
    }
    write_json(out / "distill_summary.json", summary)
    print("[distill] %s" % trained_marker)
    print("[distill] saved %s" % final)
    print("[distill] metrics -> %s" % log_path)
    return 0


def prepare_student_for_training(args: argparse.Namespace, teacher: nn.Module,
                                  tcfg: Any, device: torch.device
                                  ) -> Tuple[nn.Module, Any, str]:
    """Load the student checkpoint if we have one, else build it from the teacher."""
    if args.student:
        p = Path(args.student)
        if not p.exists():
            raise UserError("student checkpoint not found: %s" % p)
        try:
            st = load_student(p)
        except UserError as e:
            print("[student] WARNING: %s -> building a fresh student from the teacher"
                  % e)
            st = None
        if st is not None:
            return st.to(device), model_config(st), "checkpoint:%s" % p
    out = Path(args.out) if args.out else Path(".")
    auto = out / "student.bitfuzzy"
    if auto.exists():
        try:
            st = load_student(auto)
            return st.to(device), model_config(st), "checkpoint:%s" % auto
        except UserError as e:
            print("[student] WARNING: %s -> building a fresh student from the teacher"
                  % e)
    return build_student_from_teacher(args, teacher, tcfg, device)


def build_student_from_teacher(args: argparse.Namespace, teacher: nn.Module,
                               tcfg: Any, device: torch.device
                               ) -> Tuple[nn.Module, Any, str]:
    """Build a student from the teacher: K steps over one weight-shared block."""
    cands, survivors, _n, _o = load_or_compute_candidates(args, teacher, tcfg, device)
    picked = evenly_spaced(survivors, int(args.blocks))
    student, scfg = build_student(tcfg, len(picked), args.latents, args.seq_len,
                                  args.experts, args.topk_experts, len(picked),
                                  not args.no_act_halting)
    student.to(device)
    dense_layers = find_blocks(teacher)
    sblocks = find_blocks(student)
    if picked and dense_layers and sblocks:
        src = picked[len(picked) // 2]
        if src < len(dense_layers):
            copy_block_weights(dense_layers[src][1], sblocks[0][1])
    copy_embeddings(teacher, student)
    init_step_embedding(student)
    return student, scfg, "fresh-from-teacher (candidates=%s, sources=%s)" % (cands, picked)


# ---------------------------------------------------------------------------
# Phase 6: low-rank
# ---------------------------------------------------------------------------

def is_expert_projection(name: str) -> bool:
    low = name.lower()
    return any(k in low for k in ("expert", "moe", "sugeno")) and "router" not in low


def _effective_weight(mod: nn.Module) -> Optional[torch.Tensor]:
    """The weight the layer really uses, as a matrix.

    BitLinear:      W_eff = trits * row_scale
    SugenoBitLinear: W_eff = diag(mask) @ (trits * row_scale)
                    (the Sugeno mask selects the INPUT features before the
                    linear, so the effective matrix carries the mask).
    """
    w = getattr(mod, "weight", None)
    if not isinstance(w, torch.Tensor):
        return None
    tw = getattr(mod, "ternary_weight", None)
    if not callable(tw):
        return w.detach().float()
    with torch.no_grad():
        eff = tw()
        rs = getattr(mod, "row_scale", None)
        if callable(rs):
            eff = eff * rs()
        mask = getattr(mod, "mask", None)
        if isinstance(mask, torch.Tensor):
            mt = getattr(mod, "mask_trits", None)
            if callable(mt):
                eff = eff * mt().view(1, -1)
            else:
                eff = eff * fuzzy_quantize_(mask.view(1, -1)).view(1, -1)
        return eff.detach().float()


def _svd_factors(mat: torch.Tensor, rank: int) -> Tuple[torch.Tensor, torch.Tensor]:
    """Plain low-rank A x B from the SVD of mat (A: out x r, B: r x in)."""
    u, s, vh = torch.linalg.svd(mat, full_matrices=False)
    r = int(min(rank, s.numel()))
    root = torch.sqrt(s[:r].clamp_min(0))
    a = u[:, :r] * root.unsqueeze(0)
    b = root.unsqueeze(1) * vh[:r, :]
    return a, b


def apply_lowrank(student: nn.Module, rank: int) -> Dict[str, Any]:
    """SVD low-rank A x B on the MoE expert projection matrices.

    Plain low rank, NOT a Kronecker product: W ~= A @ B with A (out x r) and
    B (r x in).  The factors are attached through the module's OWN low-rank
    path (lora_a / lora_b, i.e. y = W_ternary @ x + A @ (B @ x)), because the
    forward RE-TERNARIZES `weight` on every call: writing a low-rank product
    into `weight` would be quantized back to trits and have NO effect.

    The correction is fitted as the residual of the SVD truncation against the
    layer's real effective weight, and the error is MEASURED before and after
    the ternary re-freeze, never assumed.
    """
    report: List[Dict[str, Any]] = []
    for name, mod in list(student.named_modules()):
        if not is_expert_projection(name):
            continue
        w_eff = _effective_weight(mod)
        if w_eff is None or w_eff.dim() != 2:
            continue
        out_f, in_f = w_eff.shape
        r = int(min(rank, out_f, in_f))
        if r <= 0:
            continue
        if not hasattr(mod, "lora_a") or not hasattr(mod, "lora_b"):
            report.append({"tensor": name, "skipped": "layer has no low-rank path"})
            continue
        with torch.no_grad():
            a, b = _svd_factors(w_eff, r)
            w_low = a @ b
            err_svd = float(torch.linalg.norm(w_low - w_eff)
                            / (torch.linalg.norm(w_eff) + 1e-9))
            # Residual against what the layer will still compute (trits*scale
            # and the Sugeno mask).  The low-rank correction is added to the
            # RAW input, so it is a plain additive matrix on the effective one.
            ternary_part = _effective_weight(mod)
            if ternary_part is None:
                ternary_part = w_eff
            resid = w_low - ternary_part
            ra, rb = _svd_factors(resid, r)
            mod.lora_a = nn.Parameter(ra.detach().clone())
            mod.lora_b = nn.Parameter(rb.detach().clone())
            new_eff = ternary_part + ra @ rb
            err_final = float(torch.linalg.norm(new_eff - w_eff)
                              / (torch.linalg.norm(w_eff) + 1e-9))
        report.append({
            "tensor": name,
            "shape": [out_f, in_f],
            "rank": r,
            "rel_err_svd_truncation": err_svd,
            "rel_err_after_lowrank": err_final,
            "factor_bytes_2bit": (ra.numel() + rb.numel() + 3) // 4,
            "full_bytes_2bit": (w_eff.numel() + 3) // 4,
        })
    return {"modules": report, "rank": int(rank)}


def refreeze_ternary(student: nn.Module) -> Dict[str, Any]:
    """Re-freeze every ternary master weight after the low-rank edit.

    The forward ternarizes `weight` on every call, so baking the trits into
    the stored weight only pins the scale (the row absmean of the trits).  The
    low-rank correction is re-fitted afterwards by the caller order:
    freeze FIRST, then fit the residual, so the freeze does not eat capacity.
    """
    touched = 0
    for name, p in student.named_parameters():
        low = name.lower()
        if not p.is_floating_point() or p.dim() != 2:
            continue
        if not any(k in low for k in ("attn", "q_proj", "k_proj", "v_proj", "o_proj",
                                      "gate_proj", "up_proj", "down_proj",
                                      "expert", "sugeno", "mlp")):
            continue
        with torch.no_grad():
            p.copy_(fuzzy_quantize_(p).to(dtype=p.dtype))
        touched += 1
    return {"tensors_refrozen": touched}


def block_output_probe(student: nn.Module, ids: torch.Tensor, device: torch.device
                       ) -> Optional[float]:
    """Measure the block output magnitude (before/after the low-rank patch).

    Honesty check: a low-rank that is silently a no-op (for instance because
    the layer re-ternarizes `weight` on every call) must be visible, not
    hidden behind a zero error metric.
    """
    blocks = find_blocks(student)
    if not blocks:
        return None
    blk = blocks[0][1]
    emb = getattr(student, "embed_tokens", None)
    if emb is None:
        return None
    try:
        with torch.no_grad():
            h = emb(ids.to(device))
            out = blk(h)
        if isinstance(out, tuple):
            out = out[0]
        return float(out.abs().max())
    except Exception:  # noqa: BLE001 - the probe is informational
        return None


def cmd_lowrank(args: argparse.Namespace) -> int:
    preflight_teacher(args)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    student, scfg, origin = load_or_build_student_for_edit(args, device)
    # The saved config must match the state dict: the low-rank path only
    # exists when the layers were built with lowrank_rank > 0.
    cfg = model_config(student)
    if cfg is not None and hasattr(cfg, "lowrank_rank"):
        try:
            cfg.lowrank_rank = int(args.rank)
        except Exception:  # noqa: BLE001
            pass
    before = footprint(student)
    print("[lowrank] student=%s rank=%d" % (origin, args.rank))
    probe_ids = make_batches(args, 128, 1, args.seed)[0].to(device)
    ff = refreeze_ternary(student)
    probe_before = block_output_probe(student, probe_ids, device)
    lr = apply_lowrank(student, int(args.rank))
    probe_after = block_output_probe(student, probe_ids, device)
    after = footprint(student)
    out = out_dir(Path(args.out))
    ckpt = save_student(student, out / "student_lr.bitfuzzy")
    effect = None
    if probe_before is not None and probe_after is not None:
        effect = abs(probe_after - probe_before)
    sidecar = {
        "phase": "lowrank",
        "trained": False,
        "synthetic": bool(args.synthetic),
        "rank": int(args.rank),
        "method": "plain low-rank A x B (SVD), attached through the layer's "
                  "lora_a/lora_b path. NOT a Kronecker product.",
        "student_origin": origin,
        "lowrank": lr,
        "block_output_abs_max_before": probe_before,
        "block_output_abs_max_after": probe_after,
        "block_output_delta": effect,
        "effect_note": "the block output is re-measured after the patch; a delta "
                       "of 0.0 would mean the low-rank did not reach the model.",
        "refreeze": ff,
        "footprint_before": before,
        "footprint_after": after,
        "checkpoint": str(ckpt),
    }
    write_json(out / "lowrank_report.json", sidecar)
    for m in lr["modules"]:
        if "skipped" in m:
            print("[lowrank] %s SKIPPED: %s" % (m.get("tensor"), m["skipped"]))
            continue
        print("[lowrank] %s %s rank=%d rel_err_svd=%.4f rel_err_final=%.4f"
              % (m.get("tensor"), m.get("shape"), m.get("rank", 0),
                 m.get("rel_err_svd_truncation", float("nan")),
                 m.get("rel_err_after_lowrank", float("nan"))))
    print("[lowrank] re-froze %d tensor(s) to ternary" % ff["tensors_refrozen"])
    if effect is not None:
        print("[lowrank] block output |max| before=%.6f after=%.6f delta=%.6f"
              % (probe_before, probe_after, effect))
        if effect == 0.0:
            print("[lowrank] WARNING: the low-rank did NOT change the block output.",
                  file=sys.stderr)
    print("[lowrank] saved %s" % ckpt)
    return 0


def load_or_build_student_for_edit(args: argparse.Namespace, device: torch.device
                                   ) -> Tuple[nn.Module, Any, str]:
    if args.student:
        p = Path(args.student)
        if not p.exists():
            raise UserError("student checkpoint not found: %s" % p)
        st = load_student(p)
        return st.to(device), model_config(st), "checkpoint:%s" % p
    out = Path(args.out) if args.out else Path(".")
    auto = out / "student.bitfuzzy"
    if auto.exists():
        st = load_student(auto)
        return st.to(device), model_config(st), "checkpoint:%s" % auto
    # No checkpoint: build a fresh student from the teacher so the phase is
    # runnable standalone (the CI chain runs each phase on its own).
    teacher, tcfg, _params, source = resolve_teacher(args, device)
    student, scfg, origin = build_student_from_teacher(args, teacher, tcfg, device)
    return student, scfg, origin


# ---------------------------------------------------------------------------
# Phase 7: report
# ---------------------------------------------------------------------------


def cmd_report(args: argparse.Namespace) -> int:
    preflight_teacher(args)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    teacher = None
    tcfg = None
    tparams: Dict[str, Any] = {}
    tsource = "none"
    if args.synthetic or args.dense_dir:
        teacher, tcfg, tparams, tsource = resolve_teacher(args, device)
        require_cuda(args, tcfg, student_blocks=0)
    student = None
    ssource = "none"
    if args.student:
        p = Path(args.student)
        if not p.exists():
            raise UserError("student checkpoint not found: %s" % p)
        student = load_student(p).to(device)
        ssource = "checkpoint:%s" % p
    elif args.synthetic:
        student, _scfg, ssource = load_or_build_student_for_edit(args, device)
    if student is None:
        raise UserError(
            "report needs a student: pass --student <checkpoint> (or --synthetic to "
            "build the tiny one)"
        )
    vocab = int(cfg_get(tcfg, "vocab_size", 0) or 0)
    if vocab <= 0:
        vocab = int(cfg_get(model_config(student), "vocab_size", 128) or 128)
    sample = make_batches(args, vocab, 1, args.seed)[0].to(device)
    payload: Dict[str, Any] = {
        "phase": "report",
        "trained": not bool(args.synthetic),
        "trained_marker": "trained: false (synthetic)" if args.synthetic
                          else "trained: true",
        "synthetic": bool(args.synthetic),
        "target_bytes": TARGET_BYTES,
        "teacher_source": tsource,
        "student_source": ssource,
    }
    if teacher is not None:
        payload["dense"] = {
            "config": tparams,
            "footprint": footprint(teacher),
            "compute": count_macs(teacher, sample, device),
            "checkpoint_bytes": _dir_size(args.dense_dir) if args.dense_dir else None,
        }
    payload["student"] = {
        "footprint": footprint(student),
        "compute": count_macs(student, sample, device),
        "checkpoint_bytes": _file_size(Path(args.student)) if args.student else None,
        "lowrank": _lowrank_footprint(student),
    }
    sfoot = payload["student"]["footprint"]
    proj = projected_lowrank(student, int(args.rank))
    sfoot["lowrank_projection"] = proj
    sfoot["bytes_2bit_projected_without_lowrank"] = sfoot["bytes_2bit_projected"]
    sfoot["bytes_2bit_projected_with_lowrank"] = (
        sfoot["bytes_2bit_projected"]
        - proj["expert_bytes_2bit_without"]
        + proj["expert_bytes_2bit_with"]
    )
    payload["meets_500mb_target"] = bool(
        sfoot["bytes_2bit_projected_with_lowrank"] <= TARGET_BYTES)
    payload["meets_500mb_target_fp32"] = bool(sfoot["bytes_fp32"] <= TARGET_BYTES)
    payload["module_footprint_report"] = module_footprint_report(student)
    out = Path(args.out) if args.out else Path("budget.json")
    write_json(out, payload)
    print_footprint_table(payload)
    print("[report] wrote %s" % out)
    return 0


def _file_size(p: Optional[Path]) -> Optional[int]:
    if p is None:
        return None
    try:
        return int(p.stat().st_size)
    except OSError:
        return None


def _dir_size(d: Optional[str]) -> Optional[int]:
    if not d:
        return None
    root = Path(d)
    if not root.is_dir():
        return None
    total = 0
    for p in root.rglob("*"):
        if p.is_file():
            total += p.stat().st_size
    return int(total)


def _lowrank_footprint(student: nn.Module) -> Dict[str, Any]:
    """Footprint of the low-rank factors (the deployable form)."""
    a_bytes = 0
    b_bytes = 0
    count = 0
    for name, b in student.named_parameters():
        if name.endswith("lora_a") or name.endswith("lora_b"):
            if b is None or not isinstance(b, torch.Tensor):
                continue
            n = int(b.numel())
            sz = (n + 3) // 4 if b.dtype == torch.float32 else n * b.element_size()
            if name.endswith("lora_a"):
                a_bytes += sz
            else:
                b_bytes += sz
            count += 1
    return {
        "factorised_modules": count // 2 if count else 0,
        "factor_bytes_2bit": a_bytes + b_bytes,
        "note": "A x B factors are the deployable correction; the checkpoint ALSO "
                "keeps the ternary master weights, so the file size on disk does "
                "not shrink until the runtime consumes the factors.",
    }


def projected_lowrank(student: nn.Module, rank: int) -> Dict[str, Any]:
    """Projected 2-bit footprint of the student WITH vs WITHOUT low-rank.

    Without low-rank the expert projection is a dense ternary matrix
    (out x in) packed 4 weights/byte.  With low-rank it becomes the pair
    (A: out x r, B: r x in), which is the SAME plain A x B factorization the
    lowrank phase fits (NOT a Kronecker product).
    """
    without = 0
    with_lr = 0
    modules = 0
    for name, mod in student.named_modules():
        w = getattr(mod, "weight", None)
        if not isinstance(w, torch.Tensor) or w.dim() != 2:
            continue
        if not is_expert_projection(name):
            continue
        out_f, in_f = int(w.shape[0]), int(w.shape[1])
        r = int(min(rank, out_f, in_f))
        without += (out_f * in_f + 3) // 4
        with_lr += ((out_f * r + r * in_f) + 3) // 4
        modules += 1
    return {
        "rank": int(rank),
        "expert_modules": modules,
        "expert_bytes_2bit_without": without,
        "expert_bytes_2bit_with": with_lr,
    }


def print_footprint_table(payload: Dict[str, Any]) -> None:
    print("=" * 78)
    print("FOOTPRINT / COMPUTE BUDGET  (%s)" % payload.get("trained_marker", ""))
    print("=" * 78)
    hdr = "%-12s %14s %12s %12s" % ("component", "params", "fp32", "2-bit")
    for key in ("dense", "student"):
        if key not in payload:
            continue
        block = payload[key]
        fp = block["footprint"]
        mac = block["compute"]
        print("-- %s (%s)" % (key, payload.get("%s_source" % key, "")))
        print(hdr)
        for comp, vals in fp["components"].items():
            print("%-12s %14s %12s %12s"
                  % (comp, human_params(vals["params"]),
                     fmt_bytes(vals["bytes_fp32"]), fmt_bytes(vals["bytes_2bit"])))
        print("%-12s %14s %12s %12s"
              % ("TOTAL", human_params(fp["total_params"]),
                 fmt_bytes(fp["bytes_fp32"]), fmt_bytes(fp["bytes_2bit_projected"])))
        print("   runtime cache (buffers, not weights): %s %s"
              % (fmt_bytes(fp.get("runtime_buffer_bytes", 0)),
                 fp.get("runtime_buffers", {})))
        print("   MACs/token: %s   (blocks measured: %d, tokens: %d)"
              % (human_params(int(mac["macs_per_token"])), mac["blocks_measured"],
                 mac["tokens_measured"]))
        if block.get("checkpoint_bytes"):
            print("   real artifact on disk: %s"
                  % fmt_bytes(block["checkpoint_bytes"]))
        lr = block.get("lowrank")
        if lr:
            print("   low-rank factors (2-bit): %s over %d module(s)"
                  % (fmt_bytes(lr["factor_bytes_2bit"]),
                     lr["factorised_modules"]))
        proj = fp.get("lowrank_projection")
        if proj:
            print("   low-rank projection (rank %d, %d expert module(s)): "
                  "without %s -> with %s"
                  % (proj["rank"], proj["expert_modules"],
                     fmt_bytes(proj["expert_bytes_2bit_without"]),
                     fmt_bytes(proj["expert_bytes_2bit_with"])))
        print("")
    if "student" in payload:
        sf = payload["student"]["footprint"]
        print("target <500MB: 2-bit projection %s (without low-rank) / %s (with "
              "low-rank) -> %s | fp32 %s -> %s"
              % (fmt_bytes(sf["bytes_2bit_projected_without_lowrank"]),
                 fmt_bytes(sf["bytes_2bit_projected_with_lowrank"]),
                 "MET" if payload["meets_500mb_target"] else "NOT MET",
                 fmt_bytes(sf["bytes_fp32"]),
                 "MET" if payload["meets_500mb_target_fp32"] else "NOT MET"))
    print("=" * 78)


# ---------------------------------------------------------------------------
# Dry-run plan
# ---------------------------------------------------------------------------

def print_plan(args: argparse.Namespace) -> None:
    cmd = args.command or "all"
    print("=" * 78)
    print("DRY RUN - plan only, nothing is trained")
    print("=" * 78)
    print("command      : %s" % cmd)
    print("teacher      : %s"
          % ("synthetic (tiny random teacher)"
             if args.synthetic else (args.dense_dir or "<missing --dense-dir>")))
    print("output       : %s" % (args.out or "<default>"))
    phases = {
        "angular": ["load dense teacher", "capture per-layer hidden states",
                    "angular distance d = 1 - cos(h_l, h_l+1)",
                    "mean + variance per layer",
                    "select the lowest-variance %d%% as prune candidates"
                    % int(args.prune_ratio * 100),
                    "write JSON report"],
        "collapse": ["load dense teacher", "angular analysis (or --angular-report)",
                     "drop pruned layers",
                     "K recurrent steps over ONE weight-shared block (K=%d)" % args.blocks,
                     "seed the shared block from an evenly spaced survivor",
                     "COPY weights (never average ternary weights)",
                     "init the ternary step-embedding table",
                     "save_bitfuzzy + student_config.json sidecar"],
        "distill": ["frozen dense teacher (eval, no grad)",
                    "student = BitFuzzyRecurrent",
                    "loss = alpha*KL(top-k, T) + beta*CE + gamma*sparsity",
                    "optional latent path (--latents %d)" % args.latents,
                    "AdamW + cosine LR + grad clip %.1f" % args.clip,
                    "checkpoint every %d steps + JSONL metrics" % args.save_every],
        "lowrank": ["load student", "SVD low-rank A x B on the expert gate/up/down "
                    "(rank %d)" % args.rank,
                    "re-freeze ternary", "save"],
        "report": ["load teacher + student", "count real tensors",
                   "2-bit (4/byte) and fp32 bytes", "MACs/token by hook counting",
                   "write JSON budget + print table"],
        "all": ["angular", "collapse", "distill", "lowrank", "report"],
    }
    print("phases:")
    for p in phases.get(cmd, phases["all"]):
        print("  - %s" % p)
    cfg: Any = None
    if args.synthetic:
        cfg = dict(SYNTH_DENSE)
        print("teacher config (synthetic): %s" % cfg)
    elif args.dense_dir:
        d = Path(args.dense_dir)
        if not d.is_dir():
            sys.stdout.flush()
            print("ERROR: dense model dir not found: %s" % d, file=sys.stderr)
            print("       Expected a directory with model safetensors + config.json.",
                  file=sys.stderr)
            raise SystemExit(2)
        raw = read_hf_config_json(d)
        cfg = dense_params_from_hf(raw)
        print("teacher config (from %s/config.json): %s" % (d, cfg))
    if cfg:
        est = vram_breakdown(cfg, args.batch_size, args.seq_len, args.blocks)
        print("VRAM estimate: %.2f GB  (teacher bf16 %s | student fp32 %s | "
              "AdamW %s | activations %s)"
              % (est["total_gb"], fmt_bytes(est["teacher_bf16_bytes"]),
                 fmt_bytes(est["student_fp32_bytes"]),
                 fmt_bytes(est["adam_states_bytes"]),
                 fmt_bytes(est["activations_bytes"])))
        print("estimate_vram_gb(cfg) = %.2f" % estimate_vram_gb(
            cfg, args.batch_size, args.seq_len, args.blocks))
        if not args.synthetic and not torch.cuda.is_available():
            print("NOTE: a real run needs CUDA; this host has none -> the real run "
                  "would exit with the VRAM estimate above.")
    print("expected outputs:")
    print("  %s" % (args.out or "<default>"))
    if args.log:
        print("  %s (JSONL metrics)" % args.log)
    print("=" * 78)


def read_hf_config_json(dense_dir: Path) -> Dict[str, Any]:
    p = dense_dir / "config.json"
    if not p.is_file():
        raise UserError(
            "config.json not found in %s (needed to size the run without loading "
            "the weights)" % dense_dir
        )
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        raise UserError("could not parse %s: %s" % (p, e)) from None


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="train_distill.py",
        description="Trainer for the BitFuzzy recurrent MoE student distilled "
                    "from a dense Falcon3 teacher.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("command", nargs="?", choices=[
        "angular", "collapse", "distill", "lowrank", "report"],
        help="phase to run")
    p.add_argument("--dense-dir", default=None,
                   help="directory with the dense teacher (safetensors + config.json)")
    p.add_argument("--synthetic", action="store_true",
                   help="use the tiny random teacher (CI path, runs on CPU)")
    p.add_argument("--data", default=None,
                   help="txt file with the validation/training text (byte tokenizer)")
    p.add_argument("--out", default=None,
                   help="output file (angular/report) or directory (collapse/distill/lowrank)")
    p.add_argument("--student", default=None,
                   help="student checkpoint to load (distill/lowrank/report)")
    p.add_argument("--angular-report", default=None,
                   help="reuse the prune candidates from an angular report")
    p.add_argument("--log", default=None, help="JSONL metrics file (distill)")
    p.add_argument("--steps", type=int, default=100, help="distill steps")
    p.add_argument("--batch-size", type=int, default=2, help="distill batch size")
    p.add_argument("--lr", type=float, default=1e-4, help="learning rate")
    p.add_argument("--wd", type=float, default=0.01, help="AdamW weight decay")
    p.add_argument("--clip", type=float, default=1.0, help="grad-norm clip")
    p.add_argument("--alpha", type=float, default=1.0, help="KL weight")
    p.add_argument("--beta", type=float, default=1.0, help="cross-entropy weight")
    p.add_argument("--gamma", type=float, default=0.01, help="routing sparsity weight")
    p.add_argument("--latents", type=int, default=4,
                   help="latent-thought compression size (0 disables the latent path)")
    p.add_argument("--latent-weight", type=float, default=1.0,
                   help="weight of the latent-path CE loss")
    p.add_argument("--topk", type=int, default=64, help="top-k logits for the KL")
    p.add_argument("--temperature", type=float, default=1.0, help="KL temperature")
    p.add_argument("--blocks", type=int, default=5,
                   help="K recurrent steps over the shared block")
    p.add_argument("--rank", type=int, default=64, help="low-rank rank")
    p.add_argument("--prune-ratio", type=float, default=0.3,
                   help="fraction of layers considered for pruning")
    p.add_argument("--seq-len", type=int, default=128, help="sequence length")
    p.add_argument("--val-samples", type=int, default=4,
                   help="validation sequences used by the angular analysis")
    p.add_argument("--experts", type=int, default=4, help="MoE experts in the student")
    p.add_argument("--topk-experts", type=int, default=2,
                   help="experts activated per token in the student MoE")
    p.add_argument("--no-act-halting", action="store_true",
                   help="disable the ACT-style early stop in the student")
    p.add_argument("--save-every", type=int, default=0,
                   help="checkpoint every N steps (0 = only at the end)")
    p.add_argument("--seed", type=int, default=1234, help="deterministic seed")
    p.add_argument("--dry-run", action="store_true",
                   help="print the plan and exit without training")
    return p


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command is None and not args.dry_run:
        print("ERROR: no subcommand given.", file=sys.stderr)
        print("       choose one of: angular, collapse, distill, lowrank, report",
              file=sys.stderr)
        return 2
    if args.dry_run:
        print_plan(args)
        return 0
    set_seed(int(args.seed))
    try:
        if args.command == "angular":
            return cmd_angular(args)
        if args.command == "collapse":
            return cmd_collapse(args)
        if args.command == "distill":
            return cmd_distill(args)
        if args.command == "lowrank":
            return cmd_lowrank(args)
        if args.command == "report":
            return cmd_report(args)
    except UserError as e:
        print("ERROR: %s" % e, file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("ERROR: interrupted", file=sys.stderr)
        return 130
    print("ERROR: unknown command %r" % args.command, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())

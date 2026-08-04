#!/usr/bin/env python3
"""validate_hw_expert_v4_class.py — validate the EXPORTED class-v2 artifact.

Same Rust-exact loader/predictor port as validate_hw_expert_v4.py, adapted to
the relabeled GENERIC-class taxonomy:
  - family head has 12 classes (not 17)
  - labels come from models/hw_expert/v4/dataset_class_v2.json
  - test gate: >= 3 distinct families across the 10 canonical devices
  - family gate: holdout device-level acc >= 70%

Usage:
    python tools/validate_hw_expert_v4_class.py <path-to-bitnet>
"""
from __future__ import annotations

import json
import math
import struct
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
DATASET = ROOT / "models" / "hw_expert" / "v4" / "dataset_class_v2.json"
VOCAB = ROOT / "models" / "hw_expert" / "v4" / "vocab_class_v2.json"
N_CAPS = 10
# Class-v2 file layout: family head has 12 columns (the 12 generic families in
# vocab_class_v2.json). The kernel loader is updated to heads=[12,8,9,10,9] by
# a later lane; this validator is the independent Rust-exact port (12-col).
N_FAMILY_FILE = 12
N_FAMILY = 12
FAMILY_GATE = 70.0
TEST_GATE = 3

# ─── Vocab (mirror of vocab_class_v2.json) ────────────────────────────────
_voc = json.loads(VOCAB.read_text(encoding="utf-8"))
FAMILY = _voc["family"]
FW = _voc["fw"]
AGENT = _voc["agent"]
NEXT = _voc["next"]

TEST_DEVICES = [
    (0x8086, 0x100E, "8086:100e"),
    (0x1234, 0x1111, "1234:1111"),
    (0x8086, 0x1237, "8086:1237"),
    (0x8086, 0x7000, "8086:7000"),
    (0x8086, 0x7010, "8086:7010"),
    (0x8086, 0x7113, "8086:7113"),
    (0x1AF4, 0x1000, "1af4:1000"),
    (0x8086, 0x2723, "8086:2723"),
    (0x168C, 0x003E, "168c:003e"),
    (0x10EC, 0x8139, "10ec:8139"),
    (0x1B36, 0x000D, "1b36:000d"),
]

BACKBONE_KEYS = ("q", "k", "v", "o", "gate", "up", "down")


# ─── Rust-exact scalar loader (port of load_hwexpert_v5) ─────────────────
class OOB(Exception):
    pass


def ru16(d, off):
    if off + 2 > len(d):
        raise OOB()
    return struct.unpack_from("<H", d, off)[0], off + 2


def ru32(d, off):
    if off + 4 > len(d):
        raise OOB()
    return struct.unpack_from("<I", d, off)[0], off + 4


def rf32(d, off):
    if off + 4 > len(d):
        raise OOB()
    return struct.unpack_from("<f", d, off)[0], off + 4


def read_prefixed_ternary(d, off, rows, cols):
    _len, off = ru32(d, off)
    _scale, off = ru32(d, off)
    packed = (rows * cols + 3) // 4
    if off + packed > len(d):
        raise OOB()
    raw = d[off:off + packed]
    off += packed
    w = []
    for i in range(rows * cols):
        b = raw[i // 4]
        bits = (b >> ((i % 4) * 2)) & 0b11
        w.append(1.0 if bits == 0b01 else (-1.0 if bits == 0b10 else 0.0))
    return w, off


def read_prefixed_f32_vec(d, off, n):
    _len, off = ru32(d, off)
    if off + n * 4 > len(d):
        raise OOB()
    v = list(struct.unpack_from("<%df" % n, d, off))
    off += n * 4
    return v, off


def load_v5(data):
    off = 0
    magic, off = ru32(data, off)
    if magic != 0xBE11BE11:
        return None, "magic"
    version, off = ru16(data, off)
    if version < 5:
        return None, "version<5"
    _np, off = ru32(data, off)
    hidden, off = ru16(data, off)
    num_layers, off = ru16(data, off)
    off = 14
    num_heads, off = ru16(data, off)
    vocab, off = ru32(data, off)
    _ms, off = ru16(data, off)
    intermediate, off = ru16(data, off)
    _nkv, off = ru16(data, off)
    q_dim, off = ru16(data, off)
    _medusa, off = ru32(data, off)
    tie = data[off:off + 4]
    off += 4
    tie = (tie == b"MH\x00\x00")
    tok_type = data[off]
    off += 1
    tok_len, off = ru32(data, off)
    off += tok_len
    layer_features = data[off]
    off += 1
    has_inner = bool(layer_features & 0x01)
    has_ffn_ln = bool(layer_features & 0x02)
    kv_head_dim = q_dim // max(num_heads, 1)
    embed, off = read_prefixed_ternary(data, off, hidden, vocab)
    layers = []
    for _i in range(num_layers):
        rms_attn, off = read_prefixed_f32_vec(data, off, hidden)
        rms_ffn, off = read_prefixed_f32_vec(data, off, hidden)
        rms_inner, off = (read_prefixed_f32_vec(data, off, hidden) if has_inner
                          else ([1.0] * (kv_head_dim * num_heads), off))
        rms_ffn_norm, off = (read_prefixed_f32_vec(data, off, intermediate) if has_ffn_ln
                             else ([1.0] * intermediate, off))
        q, off = read_prefixed_ternary(data, off, hidden, hidden)
        k, off = read_prefixed_ternary(data, off, hidden, hidden)
        v, off = read_prefixed_ternary(data, off, hidden, hidden)
        o, off = read_prefixed_ternary(data, off, hidden, hidden)
        gate, off = read_prefixed_ternary(data, off, hidden, intermediate)
        up, off = read_prefixed_ternary(data, off, hidden, intermediate)
        down, off = read_prefixed_ternary(data, off, intermediate, hidden)
        _rope, off = read_prefixed_f32_vec(data, off, 16)
        layers.append(dict(rms_attn=rms_attn, rms_ffn=rms_ffn, rms_inner=rms_inner,
                           rms_ffn_norm=rms_ffn_norm, q=q, k=k, v=v, o=o,
                           gate=gate, up=up, down=down,
                           q_dim=q_dim, intermediate=intermediate))
    rms_final, off = read_prefixed_f32_vec(data, off, hidden)
    family_head, off = read_prefixed_ternary(data, off, hidden, N_FAMILY_FILE)
    fw_head, off = read_prefixed_ternary(data, off, hidden, 8)
    agent_head, off = read_prefixed_ternary(data, off, hidden, 9)
    caps_head, off = read_prefixed_ternary(data, off, hidden, 10)
    next_head, off = read_prefixed_ternary(data, off, hidden, 9)
    return dict(hidden=hidden, num_layers=num_layers, embed=embed, layers=layers,
                rms_final=rms_final, family_head=family_head, fw_head=fw_head,
                agent_head=agent_head, caps_head=caps_head, next_head=next_head,
                vocab=vocab, q_dim=q_dim, intermediate=intermediate,
                version=version, tok_type=tok_type, tie=tie,
                layer_features=layer_features), off


# ─── Scalar predict (port of predict_hw_v4) ──────────────────────────────
def pack_vid_did(vid, did, vocab=64):
    v = vocab
    return [(vid >> 8) % v, vid % v, (did >> 8) % v, did % v]


def rms_norm(x, weight):
    n = len(x)
    ss = sum(v * v for v in x) / n
    rms = math.sqrt(ss) + 1e-6
    return [x[i] / rms * weight[min(i, len(weight) - 1)] for i in range(n)]


def swiglu(g, u):
    return [gi * (1.0 / (1.0 + math.exp(-gi))) * ui for gi, ui in zip(g, u)]


def matmul_hybrid(w, x):
    k = len(x)
    n = len(w) // k
    out = [0.0] * n
    for j in range(n):
        s = 0.0
        for t in range(k):
            wt = w[t * n + j]
            if wt == 1.0:
                s += x[t]
            elif wt == -1.0:
                s -= x[t]
        out[j] = s
    return out


def predict(model, vid, did):
    h = model["hidden"]
    tokens = pack_vid_did(vid, did, model["vocab"])
    seq = 4
    hidden_vec = [0.0] * (seq * h)
    for ti, tok in enumerate(tokens):
        if tok < model["vocab"]:
            col = tok
            for row in range(h):
                idx = col * h + row
                hidden_vec[ti * h + row] = model["embed"][idx]
    for layer in model["layers"]:
        for pos in range(seq):
            st = pos * h
            hidden_vec[st:st + h] = rms_norm(hidden_vec[st:st + h], layer["rms_attn"])
        attn_out = [0.0] * (seq * layer["q_dim"])
        for pos in range(seq):
            st = pos * h
            inp = hidden_vec[st:st + h]
            vv = matmul_hybrid(layer["v"], inp)
            oo = matmul_hybrid(layer["o"], vv)
            for j in range(layer["q_dim"]):
                attn_out[pos * layer["q_dim"] + j] = oo[j]
        for pos in range(seq):
            hs = pos * h
            a = pos * layer["q_dim"]
            for j in range(min(h, layer["q_dim"])):
                hidden_vec[hs + j] += attn_out[a + j]
            hidden_vec[hs:hs + h] = rms_norm(hidden_vec[hs:hs + h], layer["rms_ffn"])
        ffn_out = [0.0] * (seq * h)
        for pos in range(seq):
            st = pos * h
            inp = hidden_vec[st:st + h]
            g = matmul_hybrid(layer["gate"], inp)
            u = matmul_hybrid(layer["up"], inp)
            sw = swiglu(g, u)
            d = matmul_hybrid(layer["down"], sw)
            for j in range(min(h, len(d))):
                ffn_out[pos * h + j] = d[j]
        for pos in range(seq):
            hs = pos * h
            for j in range(h):
                hidden_vec[hs + j] += ffn_out[hs + j]
    for pos in range(seq):
        st = pos * h
        hidden_vec[st:st + h] = rms_norm(hidden_vec[st:st + h], model["rms_final"])
    pooled = [0.0] * h
    for pos in range(seq):
        for j in range(h):
            pooled[j] += hidden_vec[pos * h + j]
    pooled = [v / seq for v in pooled]

    def argmax(v):
        return max(range(len(v)), key=lambda i: v[i])

    fam_all = matmul_hybrid(model["family_head"], pooled)
    # 12-col family head; clamp guard (dead col can't win with 12 cols, kept
    # for parity with the future kernel decode).
    fam = argmax(fam_all)
    if fam >= N_FAMILY:
        fam = 0  # out-of-range guard → unknown, like kernel
    fw = argmax(matmul_hybrid(model["fw_head"], pooled))
    ag = argmax(matmul_hybrid(model["agent_head"], pooled))
    caps = 0
    clog = matmul_hybrid(model["caps_head"], pooled)
    for k in range(N_CAPS):
        if k < len(clog) and clog[k] > 0.0:
            caps |= 1 << k
    nx = argmax(matmul_hybrid(model["next_head"], pooled))
    return fam, fw, ag, caps, nx


# ─── Vectorized predict (identical math, numpy batch) ────────────────────
def _mflat(flat, rows, cols):
    return np.array(flat, dtype=np.float32).reshape(rows, cols)


def predict_batch(model, vids, dids):
    h = model["hidden"]
    seq = 4
    v = model["vocab"]
    tokens = np.array([pack_vid_did(a, b, v) for a, b in zip(vids, dids)], dtype=np.int64)
    emb = _mflat(model["embed"], v, h)
    H = emb[tokens]
    for layer in model["layers"]:
        ra = np.array(layer["rms_attn"], dtype=np.float32)
        rf = np.array(layer["rms_ffn"], dtype=np.float32)
        Mv = _mflat(layer["v"], h, h)
        Mo = _mflat(layer["o"], h, h)
        Mg = _mflat(layer["gate"], h, layer["intermediate"])
        Mu = _mflat(layer["up"], h, layer["intermediate"])
        Md = _mflat(layer["down"], layer["intermediate"], h)
        ss = np.mean(H * H, axis=-1, keepdims=True)
        H = H / (np.sqrt(ss) + 1e-6) * ra
        vv = H @ Mv
        oo = vv @ Mo
        qd = layer["q_dim"]
        H = H.copy()
        H[:, :, :qd] += oo[:, :, :qd]
        ss = np.mean(H * H, axis=-1, keepdims=True)
        H = H / (np.sqrt(ss) + 1e-6) * rf
        g = H @ Mg
        u = H @ Mu
        sw = g * (1.0 / (1.0 + np.exp(-g))) * u
        d = sw @ Md
        H = H + d
    ss = np.mean(H * H, axis=-1, keepdims=True)
    H = H / (np.sqrt(ss) + 1e-6) * np.array(model["rms_final"], dtype=np.float32)
    pooled = H.mean(axis=1)
    Mfam = _mflat(model["family_head"], h, N_FAMILY_FILE)
    Mfw = _mflat(model["fw_head"], h, 8)
    Mag = _mflat(model["agent_head"], h, 9)
    Mcap = _mflat(model["caps_head"], h, 10)
    Mnx = _mflat(model["next_head"], h, 9)
    fam = (pooled @ Mfam).argmax(axis=1)
    fam = np.where(fam >= N_FAMILY, 0, fam)  # dead col → unknown (kernel parity)
    fw = (pooled @ Mfw).argmax(axis=1)
    ag = (pooled @ Mag).argmax(axis=1)
    nx = (pooled @ Mnx).argmax(axis=1)
    clog = pooled @ Mcap
    caps = np.zeros(len(vids), dtype=np.int64)
    for k in range(N_CAPS):
        caps |= (clog[:, k] > 0.0).astype(np.int64) << k
    return fam, fw, ag, caps, nx


# ─── Holdout split (90/10 by unique device, seed 42) ─────────────────────
def load_samples():
    with open(DATASET, encoding="utf-8") as f:
        data = json.load(f)
    return data["samples"] if isinstance(data, dict) else data


def split_by_device(samples, frac=0.1, seed=42):
    by_dev = defaultdict(list)
    for i, s in enumerate(samples):
        by_dev[(s["meta"]["vid"], s["meta"]["did"])].append(i)
    devices = sorted(by_dev.keys())
    rng = np.random.RandomState(seed)
    rng.shuffle(devices)
    n_hold = max(1, int(round(len(devices) * frac)))
    hold_devs = set(devices[:n_hold])
    train_idx, hold_idx = [], []
    for dev in devices:
        for i in by_dev[dev]:
            (hold_idx if dev in hold_devs else train_idx).append(i)
    return train_idx, hold_idx, hold_devs, len(devices)


# ─── Validation ───────────────────────────────────────────────────────────
def nonzero_fraction(model):
    tot = 0
    nz = 0
    for layer in model["layers"]:
        for key in BACKBONE_KEYS:
            w = layer[key]
            tot += len(w)
            nz += sum(1 for x in w if x != 0.0)
    return nz / max(tot, 1)


def validate(path, quiet=False):
    data = Path(path).read_bytes()
    try:
        m, end = load_v5(data)
    except OOB:
        print(f"[FAIL] {path}: PARSE FAILED (out of bounds)")
        return None
    if m is None:
        print(f"[FAIL] {path}: header rejected ({end})")
        return None

    out = {}
    out["file"] = str(path)
    out["size"] = len(data)
    out["parse_end"] = end
    out["parse_ok"] = end == len(data)
    out["hidden"] = m["hidden"]
    out["layers"] = m["num_layers"]
    out["vocab"] = m["vocab"]
    out["q_dim"] = m["q_dim"]
    out["ff"] = m["intermediate"]
    head_cols = [len(m["family_head"]) // m["hidden"], len(m["fw_head"]) // m["hidden"],
                 len(m["agent_head"]) // m["hidden"], len(m["caps_head"]) // m["hidden"],
                 len(m["next_head"]) // m["hidden"]]
    out["heads"] = head_cols
    # Parse-alignment property: family head MUST be 12 cols (class-v2 taxonomy);
    # the kernel loader is updated to [12,8,9,10,9] by a later lane.
    out["header_ok"] = head_cols == [N_FAMILY_FILE, 8, 9, 10, 9]
    out["nz_frac"] = nonzero_fraction(m)
    out["nz_gate"] = out["nz_frac"] >= 0.01

    rows = []
    fams = set()
    tvids = [d[0] for d in TEST_DEVICES]
    tdids = [d[1] for d in TEST_DEVICES]
    vf, vfw, vag, vcaps, vnx = predict_batch(m, tvids, tdids)
    vec_consistent = True
    for k, (vid, did, name) in enumerate(TEST_DEVICES):
        fam, fw, ag, caps, nx = predict(m, vid, did)
        fams.add(fam)
        if (int(fam), int(fw), int(ag), int(caps), int(nx)) != \
           (int(vf[k]), int(vfw[k]), int(vag[k]), int(vcaps[k]), int(vnx[k])):
            vec_consistent = False
        rows.append((name, fam, FAMILY[fam], fw, FW[fw], ag, AGENT[ag], caps, nx, NEXT[nx]))
    out["test_rows"] = rows
    out["n_distinct_family"] = len(fams)
    out["test_gate"] = len(fams) >= TEST_GATE
    out["vec_consistent"] = vec_consistent

    samples = load_samples()
    _, hold_idx, hold_devs, n_devs = split_by_device(samples)
    dev_order = []
    dev_first_label = {}
    for pos, i in enumerate(hold_idx):
        dev = (samples[i]["meta"]["vid"], samples[i]["meta"]["did"])
        if dev not in dev_first_label:
            dev_order.append(dev)
            dev_first_label[dev] = (i, pos)
    vids = [d[0] for d in dev_order]
    dids = [d[1] for d in dev_order]
    fam_p, fw_p, ag_p, caps_p, nx_p = predict_batch(m, vids, dids)
    dev_acc = {"family": 0, "fw_id": 0, "agent_id": 0, "caps_bits": 0, "next_action": 0}
    for k, dev in enumerate(dev_order):
        first_i, _ = dev_first_label[dev]
        y = samples[first_i]["y"]
        dev_acc["family"] += (int(fam_p[k]) == int(y.get("family", 0)))
        dev_acc["fw_id"] += (int(fw_p[k]) == int(y.get("fw_id", 0)))
        dev_acc["agent_id"] += (int(ag_p[k]) == int(y.get("agent_id", 0)))
        dev_acc["caps_bits"] += (int(caps_p[k]) == int(y.get("caps_bits", 0)))
        dev_acc["next_action"] += (int(nx_p[k]) == int(y.get("next_action", 8)))
    n = len(dev_order)
    out["holdout_devices"] = n
    out["holdout_acc"] = {k: v / n * 100.0 for k, v in dev_acc.items()}
    out["family_gate"] = out["holdout_acc"]["family"] >= FAMILY_GATE

    if not quiet:
        print(f"  file: {path}")
        print(f"  size={out['size']} parse_end={out['parse_end']} parse_ok={out['parse_ok']}")
        print(f"  header: hidden={out['hidden']} layers={out['layers']} vocab={out['vocab']} "
              f"q_dim={out['q_dim']} ff={out['ff']} heads={out['heads']} ok={out['header_ok']}")
        print(f"  backbone nonzero fraction: {out['nz_frac'] * 100:.3f}%  (GATE >= 1%: {out['nz_gate']})")
        print("  predictions (Rust-exact scalar port):")
        for name, fam, fname, fw, fwn, ag, agn, caps, nx, nxn in rows:
            print(f"    {name}: family={fam}({fname}) fw={fw}({fwn}) agent={ag}({agn}) "
                  f"caps=0x{caps:x} next={nx}({nxn})")
        print(f"  distinct families among test devices: {out['n_distinct_family']}  "
              f"(GATE >= {TEST_GATE}: {out['test_gate']})")
        print(f"  scalar vs vectorized port predictions agree: {out['vec_consistent']}")
        print(f"  holdout (device-level, seed-42 90/10 split, {out['holdout_devices']} devices):")
        for k in ("family", "fw_id", "agent_id", "caps_bits", "next_action"):
            print(f"    {k:12s}: {out['holdout_acc'][k]:.2f}%")
        print(f"  family GATE >= {FAMILY_GATE}%: {out['family_gate']}")
    return out


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    path = sys.argv[1]
    out = validate(path)
    if out is None:
        sys.exit(1)
    ok = all([out["parse_ok"], out["header_ok"], out["nz_gate"], out["test_gate"], out["family_gate"]])
    print(f"\n  VALIDATION {'PASS' if ok else 'FAIL'} "
          f"(parse={out['parse_ok']} header={out['header_ok']} nz={out['nz_gate']} "
          f"test={out['test_gate']} family={out['family_gate']})")
    sys.exit(0 if ok else 2)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
train_router.py — supervised training of the Trinity MoE router (ADR-0083 P1).

Trains the 99x64 char-embedding + 64x7 ternary router that mirrors
`crates/cortex/src/trinity.rs` (encode / classify_intent / load_router_from_file)
and exports a byte-exact ROUTER.BITNET v3 file loadable by the kernel.

Expert index order (must match `init_trinity` registration):
    0 generator, 1 hw_control, 2 hw_identify, 3 rust_coder,
    4 disk_diag, 5 security, 6 speech_synth

Label semantics = check ORDER of the production keyword classifier that this
training distills (hw_control first, then greeting, then the loop
hw_identify -> rust_coder -> disk_diag -> security -> speech_synth -> generator).
"""

import datetime
import json
import struct
import sys
from pathlib import Path

import numpy as np

# ---------------------------------------------------------------------------
# Fixed model definition (must mirror trinity.rs exactly)
# ---------------------------------------------------------------------------
VOCAB = 99
HIDDEN = 64
N_EXPERTS = 7
EXPERT_NAMES = ["generator", "hw_control", "hw_identify", "rust_coder",
                "disk_diag", "security", "speech_synth"]
MAX_TOKENS = 32          # encode() truncates the token list to 32
BOS, EOS, CHAR_OFFSET = 0, 1, 3


def encode(text: str) -> np.ndarray:
    """Mirror of `encode` in trinity.rs: BOS + bytes 32..=126 as (b-32)+3 + EOS,
    truncated to MAX_TOKENS. Returns a (VOCAB,) count vector (token histogram),
    equivalent to summing embedding rows over the token list."""
    toks = [BOS]
    for b in text.encode("utf-8"):
        if 32 <= b <= 126:
            toks.append((b - 32) + CHAR_OFFSET)
    toks.append(EOS)
    toks = toks[:MAX_TOKENS]
    counts = np.zeros(VOCAB, dtype=np.float32)
    for t in toks:
        counts[min(t, VOCAB - 1)] += 1.0
    return counts


def encode_batch(texts):
    return np.stack([encode(t) for t in texts])


def forward(X, embed, W):
    """Mirror of classify_intent: h = sum of embed rows; normalize;
    logits = h @ ternary(W); softmax. Straight-through: forward uses the
    quantized W, gradients flow to the float W via identity."""
    h = X @ embed
    norms = np.linalg.norm(h, axis=1, keepdims=True) + 1e-8
    h = h / norms
    Wq = np.clip(np.round(W), -1.0, 1.0)
    logits = h @ Wq
    logits -= logits.max(axis=1, keepdims=True)
    ex = np.exp(logits)
    probs = ex / ex.sum(axis=1, keepdims=True)
    return probs, Wq, h, norms


# ---------------------------------------------------------------------------
# Dataset
# ---------------------------------------------------------------------------
# CURATED: realistic PT+EN utterances, labeled per the classifier check order.
# Labels follow the spec semantics exactly (hw_control first; then greeting;
# then hw_identify -> rust_coder -> disk_diag -> security -> speech_synth ->
# generator). Triggers always sit inside the first 30 chars (the tokenizer
# truncates at 30 visible chars) so the char model can actually learn them.
CURATED = [
    # ---- hw_control (checked FIRST) --------------------------------------
    ("aumenta o volume", 1),
    ("diminui o volume", 1),
    ("volume para 50 por cento", 1),
    ("mute no microfone", 1),
    ("desativa o mute", 1),
    ("ajuste o brilho da tela", 1),
    ("defina o brilho para 70", 1),
    ("brilho maximo agora", 1),
    ("set volume to 30", 1),
    ("brightness down a bit", 1),
    ("vol para 20", 1),
    ("unmute o video", 1),
    ("a tela esta com brilho baixo", 1),
    ("aumenta o brilho", 1),
    ("mute tudo", 1),
    # ---- generator (greeting, checked 2nd; + fallback topics) ------------
    ("oi jarbas", 0),
    ("ola tudo bem", 0),
    ("bom dia jarbas", 0),
    ("boa noite", 0),
    ("como vai voce", 0),
    ("hello there", 0),
    ("hey jarbas", 0),
    ("tudo bem com voce", 0),
    ("qual o tempo hoje", 0),
    ("como esta o clima", 0),
    ("previsao do tempo", 0),
    ("vai chover amanha", 0),
    ("me conta uma piada", 0),
    ("vamos conversar um pouco", 0),
    ("qual a previsao para amanha", 0),
    ("o que voce pode fazer", 0),
    # ---- hw_identify ------------------------------------------------------
    ("qual o dispositivo 8086:1234", 2),
    ("qual pci e esse 10de:1b81", 2),
    ("identifique o dispositivo usb", 2),
    ("identify the device 1234:5678", 2),
    ("qual o id do hardware", 2),
    ("qual hardware esta instalado", 2),
    ("me da o hwid da placa", 2),
    ("busca /hw para a gpu", 2),
    ("que dispositivo e 0x8086:1234", 2),
    ("identifique o hardware de video", 2),
    ("show me the pci id", 2),
    ("qual dispositivo pci 1002:67df", 2),
    ("identify the usb device model", 2),
    # ---- rust_coder -------------------------------------------------------
    ("crie um codigo para mim", 3),
    ("write a rust function", 3),
    ("implement a parser in rust", 3),
    ("codigo para ler um arquivo", 3),
    ("crie um script de backup", 3),
    ("create a function to add", 3),
    ("preciso de codigo rust", 3),
    ("write code for the kernel", 3),
    ("implemente um parser de json", 3),
    ("crie uma funcao em rust", 3),
    ("code a sorting algorithm", 3),
    ("escreve um driver de rede", 3),
    ("crie um modulo de rede", 3),
    ("me escreve uma funcao", 3),
    # ---- disk_diag --------------------------------------------------------
    ("verifica o estado do disco", 4),
    ("roda um smart check no hd", 4),
    ("analisa o armazenamento", 4),
    ("disk health report", 4),
    ("check the smart data", 4),
    ("como esta meu storage", 4),
    ("espaco livre no disco", 4),
    ("testa o disco principal", 4),
    ("smart status da unidade", 4),
    ("storage full warning", 4),
    ("o disco esta com defeito", 4),
    ("me mostra o smart do ssd", 4),
    ("armazenamento cheio", 4),
    # ---- security ---------------------------------------------------------
    ("analisa a seguranca do sistema", 5),
    ("existe algum cve no kernel", 5),
    ("security scan now", 5),
    ("verifica a seguranca da rede", 5),
    ("qual cve afeta o wifi", 5),
    ("detecta ataques na rede", 5),
    ("check for security issues", 5),
    ("alguma vulnerabilidade cve", 5),
    ("protecao contra ataque", 5),
    ("any attack detected", 5),
    ("report cve 2024 1234", 5),
    ("seguranca do servidor", 5),
    ("scan for attacks now", 5),
    # ---- speech_synth -----------------------------------------------------
    ("fale o numero 42", 6),
    ("pronuncie a palavra rust", 6),
    ("fale algo motivacional", 6),
    ("tts para o texto", 6),
    ("pronounce the word kernel", 6),
    ("fale em voz alta", 6),
    ("fale a frase do dia", 6),
    ("speak the result", 6),
    ("diga o numero do voo", 6),
    ("fale um cumprimento", 6),
    ("speak the news", 6),
    ("fale o nome do sistema", 6),
    ("pronuncie o texto todo", 6),
    # ---- deliberately tricky cross-expert (label = check order) -----------
    ("escreva um script para analisar um cve", 3),   # write before security
    ("crie um ataque de teste", 3),                  # crie before ataque
    ("fale o resultado do smart", 4),                # smart before fale
    ("pronuncie cve 2024 1234", 5),                  # cve before pronounce
    ("diga o clima de hoje", 6),                     # diga before clima topic
    ("oi fale sobre o clima", 0),                    # greeting first
    ("ajuste o volume e fale ok", 1),                # hw_control first
    ("jarbas o que e um cve", 0),                    # short jarbas -> greeting
    ("diga bom dia em ingles", 0),                   # greeting before speech
    ("qual o cve do disco", 4),                      # disco before cve
    ("speak the weather report", 6),                 # speech before topic
    ("me conta sobre o cve do disco", 4),            # disco before cve
    ("identifique o cve do disco", 4),               # disco before cve
    ("set brightness and speak", 1),                 # hw_control first
]

# ---------------------------------------------------------------------------
# Template augmentation (TRAIN volume only; deterministic seed).
# Every generated utterance must be labelable by the classifier semantics
# (the marker word must actually be present). Kept <= 30 chars so triggers
# stay inside the model's 30-char window. Per-language heads/skels avoid
# mixed-language sentences that blur the char signal.
# ---------------------------------------------------------------------------
TEMPLATE_SPEC = {
    0: {  # generator: greetings + weather/chat fallback topics
        "greets": ["oi", "ola", "hello", "hey", "hi", "bom dia", "boa tarde",
                   "boa noite", "como vai", "tudo bem", "saudacoes"],
        "topics": ["tempo", "clima", "weather", "previsao", "amanha", "hoje"],
        "skels": ["{g}", "{g} jarbas", "{g} tudo bem", "{g} como voce esta",
                  "qual o {t} hoje", "como esta o {t}", "qual a previsao do {t}",
                  "vai chover {t}", "o que acha do {t}", "conta sobre o {t}",
                  "{t} hoje", "previsao do {t} amanha", "me conta do {t}",
                  "qual {t} para amanha", "{t} agora",
                  "vamos conversar", "conversa comigo", "me conta uma historia",
                  "qual a noticia de hoje", "conta uma piada",
                  "o que voce pode fazer", "pode me ajudar"],
    },
    1: {  # hw_control: every skel contains a marker (volume/mute/brilho/...)
        "markers": ["volume", "mute", "unmute", "brilho", "brightness"],
        "pt_pairs": ["ajuste", "ajustar", "ajusta", "definir", "defina"],
        "en_pairs": ["set"],
        "levels": ["", " para 40", " para 70", " to 50", " agora",
                   " pra baixo", " to 30"],
        "pt_skels": ["{m}", "{m}{l}", "aumenta o {m}", "diminui o {m}",
                     "{p} o {m}", "preciso {p} o {m}", "quero {p} o {m}",
                     "o {m} esta alto", "o {m} esta baixo", "{m} agora"],
        "en_skels": ["{m}", "{m}{l}", "increase the {m}", "lower the {m}",
                     "change the {m}", "can you {p} the {m}",
                     "please {p} the {m}", "the {m} is high",
                     "the {m} is low", "{m} it now"],
        "extra": ["vol para 15", "vol para 60", "vol para 80", "vol para 30",
                  "vol para 100", "vol=50", "vol=20", "mute o audio",
                  "mute o som", "ativa o mute", "unmute o microfone",
                  "volume para 40", "set volume to 60",
                  "defina o brilho para 40", "ajuste o volume para 30",
                  "vol para 25", "vol para 45", "vol para 75", "vol para 90",
                  "vol para 10", "set the brightness and speak",
                  "change brightness and speak", "set volume and speak",
                  "mute and speak", "lower the brightness and speak",
                  "set brightness then speak", "set brightness and speak now",
                  "adjust brightness and speak", "brightness up then speak",
                  "set volume then speak", "desative o mute", "tira o mute",
                  "desliga o mute", "desativa o mute agora",
                  "desative o mute do video", "set brightness and speak please",
                  "set brightness and speak clearly", "mute the volume and speak",
                  "set the brightness then speak"],
    },
    2: {  # hw_identify: qualifying objects only (hex/pci/hwid//hw markers)
        "heads": ["pci", "hwid", "identifique", "identify", "hardware",
                  "dispositivo", "device", "usb"],
        "objects": ["hardware", "dispositivo", "device", "usb"],
        "hexes": ["8086:1234", "10de:1b81", "1002:67df", "1a86:7523",
                  "8086:9a60", "14e4:43a0"],
        "skels": ["qual o {o} {hex}", "qual {o} e {hex}", "{o} {hex} qual e",
                  "qual o id do {o}", "mostra o hwid do {o}",
                  "busca /hw do {o}", "identifique o {o}", "identify the {o}",
                  "show the pci id of the {o}", "which {o} is {hex}",
                  "que {o} e {hex}", "qual pci {hex}", "qual o dispositivo {hex}",
                  "me da o hwid do {o}"],
        "extra": ["procura /hw do dispositivo", "qual o /hw do hardware",
                  "busca /hw agora", "qual /hw e esse", "mostra o /hw",
                  "busca /hw do usb", "procura /hw da placa",
                  "procura /hw para a placa", "busca /hw para o pc",
                  "pesquisa /hw para o notebook", "busca /hw para a rede"],
    },
    3: {  # rust_coder: PT stems incl. escreva (task semantics), EN write/create
        "pt_heads": ["crie", "criar", "cria", "escreva", "escreve",
                     "implemente", "gere"],
        "en_heads": ["write", "code", "implement", "create"],
        "objects": ["script", "funcao", "parser", "driver", "modulo", "kernel",
                    "rust", "algoritmo", "json", "arquivo", "bootloader", "tcp",
                    "api", "handler", "teste"],
        "pt_skels": ["{h} um {o}", "{h} um {o} para mim", "preciso de um {o} {h}",
                     "pode {h} um {o}", "me {h} um {o}", "quero {h} um {o}",
                     "por favor {h} um {o}", "{h} uma funcao de {o}",
                     "me ajuda a {h} um {o}", "{h} um script em {o}"],
        "en_skels": ["{h} a {o}", "{h} a {o} for me", "i need to {h} a {o}",
                     "can you {h} a {o}", "please {h} a {o}",
                     "{h} the {o} now", "write a function to {o}",
                     "{h} code for the {o}"],
        "extra": ["escreva um script para testar um cve", "escreva um script",
                  "crie um codigo", "write a rust parser",
                  "implement a driver in rust", "codigo para ler um json",
                  "crie um script de ataque", "write a new function",
                  "create a rust function", "implement a rust function",
                  "code a rust function", "write a small function",
                  "write a rust module", "write a rust driver",
                  "write the function", "write a function for the kernel",
                  "write a parser in rust", "escreva uma funcao em rust",
                  "write a function for the network", "write a function to parse"],
    },
    4: {  # disk_diag
        "heads": ["disk", "disco", "smart", "storage", "armazenamento"],
        "objects": ["ssd", "hd", "particao", "unidade", "sistema", "dados",
                    "backup"],
        "verbs": ["verifica", "analisa", "testa", "checa", "check", "roda",
                  "run", "scan", "monitora", "escaneia"],
        "skels": ["{v} o {h}", "{v} o estado do {h}", "{v} o {h} do sistema",
                  "roda um {h} check no {o}", "{h} health now", "check the {h}",
                  "qual o status do {h}", "como esta o {h}",
                  "{h} do pc", "tem problema no {h}",
                  "{v} espaco no {h}", "{v} o {o} do {h}"],
        "extra": ["roda um smart check na unidade", "verifica o estado do storage",
                  "analisa o armazenamento do pc", "disk health now",
                  "check the smart values", "o disco esta lento",
                  "fale o status do smart", "diga o resultado do smart",
                  "qual cve afeta o disco", "procura cve no disco",
                  "fale o resultado do disco", "identifique o estado do disco",
                  "identifique o problema do disco",
                  "identifique o smart do disco",
                  "identifique o problema no disco",
                  "identifique o defeito do disco",
                  "identifique o setor do disco",
                  "identifique o estado do smart",
                  "identifique o erro do disco", "identifique o smart do hd",
                  "identifique o problema do storage",
                  "identifique o status do disco"],
    },
    5: {  # security
        "heads": ["security", "seguranca", "cve", "attack", "ataque"],
        "objects": ["kernel", "rede", "firewall", "wifi", "sistema", "server",
                    "servidor", "dispositivo", "pacote", "log", "backup"],
        "verbs": ["analisa", "verifica", "scan", "check", "detecta",
                  "monitora", "procura", "busca", "find", "report"],
        "skels": ["{v} a {h} do {o}", "{v} por {h}", "existe algum {h} em {o}",
                  "any {h} in the {o}", "check the {h} of the {o}",
                  "{h} scan now", "report {h} issues", "{v} {h} no {o}",
                  "{h} do {o}", "tem {h} no {o}", "procura {h} no log"],
        "extra": ["existe algum cve no firewall", "analisa a seguranca do servidor",
                  "run a security scan", "detecta ataques no firewall",
                  "report the cve now", "qual cve afeta o kernel",
                  "diga o cve da rede"],
    },
    6: {  # speech_synth: marker heads + a few fixed ordering templates
        "pt_heads": ["fale", "diga", "pronuncie"],
        "en_heads": ["pronounce", "speak"],
        "tts_heads": ["tts"],
        "objects": ["palavra", "word", "frase", "texto", "numero", "number",
                    "resultado", "result", "mensagem", "message", "voz",
                    "cumprimento", "nome"],
        "pt_skels": ["{h} a {o}", "{h} o {o}", "{h} a palavra {o}",
                     "{h} em voz alta", "{h} algo em ingles",
                     "{h} o texto todo", "preciso que {h} o {o}",
                     "{h} o {o} agora"],
        "en_skels": ["{h} the {o}", "{h} the {o} aloud", "can you {h} the {o}",
                     "please {h} the {o}", "{h} it now",
                     "{h} the {o} clearly"],
        "tts_skels": ["tts para o {o}", "usa tts no {o}", "tts do {o} agora"],
        "extra": ["fale o clima", "diga o tempo", "speak the weather",
                  "fale a previsao", "diga o clima amanha",
                  "fale o tempo amanha", "speak the weather now",
                  "fale o numero 7", "pronuncie a palavra kernel",
                  "speak the outcome"],
    },
}


def build_templates(seed: int = 1234):
    rng = np.random.default_rng(seed)
    samples = []
    for expert, spec in TEMPLATE_SPEC.items():
        for s in spec.get("extra", []):
            samples.append((s, expert))
        # language groups: (heads, skels, pairs)
        groups = []
        if spec.get("heads") and spec.get("skels"):
            groups.append((spec["heads"], spec["skels"], []))
        if spec.get("greets") and spec.get("skels"):
            groups.append((spec["greets"], spec["skels"], []))
        for lg in ("pt", "en", "tts"):
            heads = spec.get(f"{lg}_heads")
            skels = spec.get(f"{lg}_skels")
            pairs = spec.get(f"{lg}_pairs", [])
            if heads and skels:
                groups.append((heads, skels, pairs))
            # hw_control style: shared markers + language-specific pairs
            if spec.get("markers") and skels:
                groups.append((spec["markers"], skels, pairs))
        assert groups, f"no template groups for expert {expert}"
        greets = spec.get("greets", [])
        topics = spec.get("topics", [])
        markers = spec.get("markers", [])
        n_obj = len(spec.get("objects", [1]))
        n_hex = len(spec.get("hexes", [1]))
        n_lvl = len(spec.get("levels", [1]))
        n_verb = len(spec.get("verbs", [1]))
        for _ in range(260):
            heads, skels, pairs = groups[rng.integers(len(groups))]
            h = heads[rng.integers(len(heads))]
            sk = skels[rng.integers(len(skels))]
            o = spec.get("objects", [""])[rng.integers(n_obj)]
            hexid = spec.get("hexes", [""])[rng.integers(n_hex)]
            lvl = spec.get("levels", [""])[rng.integers(n_lvl)]
            v = spec.get("verbs", [""])[rng.integers(n_verb)]
            g = greets[rng.integers(len(greets))] if greets else ""
            t = topics[rng.integers(len(topics))] if topics else ""
            m = markers[rng.integers(len(markers))] if markers else ""
            p = pairs[rng.integers(len(pairs))] if pairs else ""
            s = (sk.replace("{h}", h).replace("{o}", o).replace("{hex}", hexid)
                    .replace("{l}", lvl).replace("{v}", v).replace("{g}", g)
                    .replace("{t}", t).replace("{m}", m).replace("{p}", p))
            s = " ".join(s.split())  # collapse double spaces
            if 4 <= len(s) <= 30:    # keep triggers inside the 30-char window
                samples.append((s, expert))
    return samples


def stratified_split(items, test_frac, seed):
    """Stratified per-expert holdout. Returns (test, rest)."""
    rng = np.random.default_rng(seed)
    by_expert = {}
    for idx, (text, lab) in enumerate(items):
        by_expert.setdefault(lab, []).append(idx)
    test_idx, rest_idx = [], []
    for lab, idxs in sorted(by_expert.items()):
        idxs = list(idxs)
        rng.shuffle(idxs)
        n_test = max(1, int(round(len(idxs) * test_frac)))
        test_idx.extend(idxs[:n_test])
        rest_idx.extend(idxs[n_test:])
    test = [items[i] for i in test_idx]
    rest = [items[i] for i in rest_idx]
    return test, rest


# ---------------------------------------------------------------------------
# Training
# ---------------------------------------------------------------------------
def adam_step(params, grads, m, v, t, lr, b1=0.9, b2=0.999, eps=1e-8):
    for p, g, mi, vi in zip(params, grads, m, v):
        mi[:] = b1 * mi + (1 - b1) * g
        vi[:] = b2 * vi + (1 - b2) * g * g
        mh = mi / (1 - b1 ** t)
        vh = vi / (1 - b2 ** t)
        p[:] -= lr * mh / (np.sqrt(vh) + eps)


def train(seed=7, lr=1e-2, l2=1e-4, epochs=500, patience=60, batch=48):
    rng = np.random.default_rng(seed)

    # --- data splits -------------------------------------------------------
    test, rest = stratified_split(CURATED, 0.28, seed=seed)
    val, train_cur = stratified_split(rest, 0.30, seed=seed + 1)
    templates = build_templates(seed=1234)

    train_items = templates + train_cur
    test_texts = {t for t, _ in test}
    train_items = [(t, l) for (t, l) in train_items if t not in test_texts]
    Xtr = encode_batch([t for t, _ in train_items])
    ytr = np.array([l for _, l in train_items], dtype=np.int64)
    Xva = encode_batch([t for t, _ in val])
    yva = np.array([l for _, l in val], dtype=np.int64)
    Xte = encode_batch([t for t, _ in test])
    yte = np.array([l for _, l in test], dtype=np.int64)
    n_classes = N_EXPERTS

    # --- model init --------------------------------------------------------
    # W at scale 1.0, NOT 0.1: round(N(0,0.1)) is all zeros, which zeroes
    # dh = dlogits @ Wq.T in the backward pass (straight-through through an
    # all-zero ternary) and kills the embedding gradient entirely — the model
    # then collapses. Scale 1.0 keeps ~60% of ternary entries nonzero at init
    # so gradient flows to embed from step 0.
    rng = np.random.default_rng(seed + 2)
    embed = rng.normal(0.0, 0.05, (VOCAB, HIDDEN)).astype(np.float32)
    W = rng.normal(0.0, 1.0, (HIDDEN, n_classes)).astype(np.float32)

    m_embed = np.zeros_like(embed)
    v_embed = np.zeros_like(embed)
    m_W = np.zeros_like(W)
    v_W = np.zeros_like(W)

    best = {"acc": -1.0, "epoch": 0, "ce": float("inf"),
            "embed": embed.copy(), "W": W.copy()}
    n_train = Xtr.shape[0]
    for epoch in range(1, epochs + 1):
        perm = rng.permutation(n_train)
        for start in range(0, n_train, batch):
            idx = perm[start:start + batch]
            Xb, yb = Xtr[idx], ytr[idx]
            probs, Wq, h, norms = forward(Xb, embed, W)
            oh = np.eye(n_classes, dtype=np.float32)[yb]
            dlogits = (probs - oh) / Xb.shape[0]
            dWq = h.T @ dlogits
            dh = dlogits @ Wq.T / norms  # straight-through on the norm too
            dX = dh @ embed.T
            d_embed = Xb.T @ dh + 2.0 * l2 * embed
            adam_step([embed, W], [d_embed, dWq],
                      [m_embed, m_W], [v_embed, v_W], epoch, lr)

        # validation every epoch (tiny data — cheap)
        pv, _, _, _ = forward(Xva, embed, W)
        ce = -np.log(pv[np.arange(len(yva)), yva] + 1e-12).mean()
        acc = (pv.argmax(1) == yva).mean()
        if acc > best["acc"] - 1e-9 and ce < best["ce"] - 1e-12:
            best = {"acc": float(acc), "epoch": epoch, "ce": float(ce),
                    "embed": embed.copy(), "W": W.copy()}
        if epoch - best["epoch"] >= patience:
            break

    pt, _, _, _ = forward(Xte, best["embed"], best["W"])
    test_acc = float((pt.argmax(1) == yte).mean())
    return {
        "embed": best["embed"], "W": best["W"],
        "test": test, "val": val, "train_cur": train_cur, "templates": templates,
        "Xte": Xte, "yte": yte, "probs": pt,
        "epochs_done": epoch, "best_epoch": best["epoch"],
        "val_acc": best["acc"], "val_ce": best["ce"],
        "test_acc": test_acc, "n_train": n_train,
    }


# ---------------------------------------------------------------------------
# Metrics
# ---------------------------------------------------------------------------
def confusion_matrix(y_true, y_pred, n=N_EXPERTS):
    cm = np.zeros((n, n), dtype=int)
    for t, p in zip(y_true, y_pred):
        cm[t, p] += 1
    return cm


def metrics(cm):
    n = cm.shape[0]
    prec = np.zeros(n)
    rec = np.zeros(n)
    f1 = np.zeros(n)
    for i in range(n):
        denom = cm[:, i].sum()
        prec[i] = cm[i, i] / denom if denom else 0.0
        denom = cm[i, :].sum()
        rec[i] = cm[i, i] / denom if denom else 0.0
        f1[i] = 2 * prec[i] * rec[i] / (prec[i] + rec[i]) if (prec[i] + rec[i]) else 0.0
    return prec, rec, f1


def print_matrix(cm, names):
    w = max(len(n) for n in names) + 2
    header = " " * (w + 1) + "".join(f"{n[:9]:>10}" for n in names) + "  |row|"
    print(header)
    print("-" * len(header))
    for i, name in enumerate(names):
        row = " ".join(f"{cm[i, j]:>10}" for j in range(len(names)))
        print(f"{name:>{w}} {row}  |{cm[i].sum():>3}|")
    colsum = " ".join(f"{cm[:, j].sum():>10}" for j in range(len(names)))
    print("-" * len(header))
    print(f"{'col':>{w}} {colsum}")


def top2(probs_row, names):
    order = np.argsort(probs_row)[::-1][:2]
    return [(names[i], float(probs_row[i])) for i in order]


# ---------------------------------------------------------------------------
# Export — byte-exact ROUTER.BITNET (v6, ADR-0085 model_type=2)
# ---------------------------------------------------------------------------
def quantize(W):
    return np.clip(np.round(W), -1.0, 1.0).astype(np.int8)


def export_bitnet(embed, W, path):
    """Export router v6 (ADR-0085 model_type=2): posicional, sem names/tags."""
    from tools.bitnet_writer import MAGIC, MODEL_ROUTER
    Wq = quantize(W)
    embed_bytes = embed.astype("<f4").tobytes()
    wbytes = Wq.tobytes()  # row-major (HIDDEN, N_EXPERTS) i8

    out = bytearray()
    # Preamble: magic + version + num_params u64 + model_type + reserved
    out += struct.pack("<I", MAGIC)         # 0:  magic
    out += struct.pack("<H", 6)             # 4:  version
    out += struct.pack("<Q", 0)             # 6:  num_params u64 (informativo)
    out += struct.pack("<B", MODEL_ROUTER)  # 14: model_type=2
    out += b"\x00\x00\x00"                 # 15: reserved
    # Router bloco (ADR-0085 §3.3)
    out += struct.pack("<I", VOCAB)         # 18: vocab u32
    out += struct.pack("<H", HIDDEN)        # 22: hidden u16
    out += struct.pack("<H", N_EXPERTS)     # 24: n_experts u16
    out += embed_bytes                      # 26: embed f32[vocab×hidden]
    out += wbytes                           #     weight i8[hidden×n_experts]

    path.write_bytes(out)
    return Wq


def verify_roundtrip(path, embed, Wq, samples):
    """Independent minimal parser — mirrors export_bitnet v6 posicional layout
    (preamble + router bloco, ADR-0085 model_type=2)."""
    data = path.read_bytes()
    assert len(data) >= 32
    # Preamble: magic u32 + version u16 + num_params u64 + model_type u8 + reserved[3]
    magic, = struct.unpack_from("<I", data, 0)
    version, = struct.unpack_from("<H", data, 4)
    model_type, = struct.unpack_from("<B", data, 14)
    assert magic == 0xBE11BE11, f"magic {magic:#x}"
    assert version == 6, f"version {version}"
    # Router bloco: vocab u32 + hidden u16 + n_experts u16
    vocab, hidden, n_exp = struct.unpack_from("<IHH", data, 18)
    assert vocab == VOCAB and hidden == HIDDEN and n_exp == N_EXPERTS, \
        f"dims {vocab}x{hidden}x{n_exp}"

    pos = 26
    embed_bytes = VOCAB * HIDDEN * 4
    embed_parsed = np.frombuffer(data[pos:pos + embed_bytes], dtype="<f4").reshape(VOCAB, HIDDEN)
    pos += embed_bytes
    wbytes = HIDDEN * N_EXPERTS
    Wq_parsed = np.frombuffer(data[pos:pos + wbytes], dtype=np.int8).reshape(HIDDEN, N_EXPERTS)
    pos += wbytes
    assert pos == len(data), f"trailing bytes {len(data) - pos}"

    assert np.array_equal(embed_parsed, embed), "embed round-trip mismatch"
    assert np.array_equal(Wq_parsed, Wq), "quantized W round-trip mismatch"

    # prediction consistency: file-parsed weights must reproduce in-memory argmax
    X = encode_batch([t for t, _ in samples])
    p_file, _, _, _ = forward(X, embed_parsed, Wq_parsed.astype(np.float32))
    p_mem, _, _, _ = forward(X, embed, Wq.astype(np.float32))
    assert np.array_equal(p_file.argmax(1), p_mem.argmax(1)), \
        "file-vs-memory argmax mismatch"
    return embed_parsed, Wq_parsed


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
def write_report(res, cm, prec, rec, f1, path):
    test_items = res["test"]
    names = EXPERT_NAMES
    lines = []
    lines.append("# Router Confusion Matrix — Trinity MoE (ADR-0083 P1)\n")
    lines.append(f"- Date: {datetime.date.today().isoformat()}")
    lines.append(f"- Expert index order: {', '.join(names)}")
    lines.append(f"- CURATED utterances: {len(CURATED)} total; "
                 f"TRAIN templates: {len(res['templates'])}")
    lines.append(f"- Split: TEST {len(res['test'])} (stratified holdout, never "
                 f"seen in training), VAL {len(res['val'])}, "
                 f"TRAIN {len(res['train_cur'])} curated + templates "
                 f"({res['n_train']} samples)")
    lines.append(f"- Training: {res['epochs_done']} epochs run, best at "
                 f"epoch {res['best_epoch']} (early stop patience 60); "
                 f"val acc {res['val_acc']:.3f}, val CE {res['val_ce']:.4f}")
    lines.append(f"- Overall accuracy (pure argmax, no threshold): "
                 f"**{res['test_acc']:.3f}** ({int(res['test_acc'] * len(test_items))}"
                 f"/{len(test_items)})\n")

    lines.append("## Confusion matrix (true x pred)\n")
    lines.append("| true \\ pred | " + " | ".join(names) + " | row |")
    lines.append("|---" * (len(names) + 1) + "|")
    for i, name in enumerate(names):
        row = " | ".join(str(cm[i, j]) for j in range(len(names)))
        lines.append(f"| **{name}** | {row} | {cm[i].sum()} |")
    lines.append("| **col** | " + " | ".join(str(cm[:, j].sum())
                                             for j in range(len(names))) + " | |\n")

    lines.append("## Per-class metrics\n")
    lines.append("| expert | precision | recall | F1 | support |")
    lines.append("|---|---|---|---|---|")
    for i, name in enumerate(names):
        lines.append(f"| {name} | {prec[i]:.3f} | {rec[i]:.3f} | {f1[i]:.3f} "
                     f"| {cm[i].sum()} |")
    lines.append(f"\nOverall accuracy: **{res['test_acc']:.3f}**\n")

    lines.append("## Mismatch highlights\n")
    y_pred = res["probs"].argmax(1)
    mis = [(t, tr, pr) for (t, tr), pr in zip(test_items, y_pred) if tr != pr]
    if not mis:
        lines.append("No misclassifications on the test holdout.")
    for text, tr, pr in mis[:10]:
        t2 = top2(res["probs"][test_items.index((text, tr))], names)
        t2s = ", ".join(f"{n}={p:.3f}" for n, p in t2)
        lines.append(f"- `{text}` — true **{names[tr]}**, pred "
                     f"**{names[pr]}** (top-2: {t2s})")
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def main():
    import sys as _sys
    # ponytail: permite `from tools.bitnet_writer` quando rodado como script (tools/ não é pacote)
    if str(Path(__file__).resolve().parents[1]) not in _sys.path:
        _sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    outdir = Path(__file__).resolve().parent / "target"
    outdir.mkdir(exist_ok=True)
    bitnet_path = outdir / "ROUTER.BITNET"
    report_path = outdir / "router_confusion_matrix.md"

    res = train()
    cm = confusion_matrix(res["yte"], res["probs"].argmax(1))
    prec, rec, f1 = metrics(cm)

    print("Trinity MoE router training (ADR-0083 P1)")
    print(f"  curated={len(CURATED)}  templates={len(res['templates'])}  "
          f"train={res['n_train']}  val={len(res['val'])}  test={len(res['test'])}")
    print(f"  epochs run={res['epochs_done']}  best_epoch={res['best_epoch']}  "
          f"val_acc={res['val_acc']:.3f}  val_ce={res['val_ce']:.4f}")
    print()
    print("Confusion matrix (true x pred):")
    print_matrix(cm, EXPERT_NAMES)
    print()
    print("Per-class metrics:")
    print(f"  {'expert':<14}{'prec':>8}{'rec':>8}{'f1':>8}{'sup':>6}")
    for i, name in enumerate(EXPERT_NAMES):
        print(f"  {name:<14}{prec[i]:>8.3f}{rec[i]:>8.3f}{f1[i]:>8.3f}{cm[i].sum():>6}")
    print(f"\n  OVERALL ACCURACY: {res['test_acc']:.3f} "
          f"({int(res['test_acc'] * len(res['test']))}/{len(res['test'])})  "
          f"gate>=0.80 -> {'PASS' if res['test_acc'] >= 0.80 else 'FAIL'}")

    Wq = export_bitnet(res["embed"], res["W"], bitnet_path)
    verify_roundtrip(bitnet_path, res["embed"], Wq, res["test"][:6])
    print(f"\nExported {bitnet_path} ({bitnet_path.stat().st_size} bytes) "
          f"— round-trip OK")
    # Canonical locations: target/ is the FAT source (mkfat32 find_file), target1 is v6 canonical.
    # Keep tools/target for backwards compat + copy to target/target1 so builds find it without --curated.
    for dest in [Path(__file__).resolve().parents[1] / "target" / "ROUTER.BITNET",
                 Path(__file__).resolve().parents[1] / "target1" / "ROUTER.BITNET"]:
        try:
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(bitnet_path.read_bytes())
            print(f"  -> copied to {dest}")
        except Exception as e:
            print(f"  [warn] copy to {dest} failed: {e}")

    write_report(res, cm, prec, rec, f1, report_path)
    print(f"Report: {report_path}")
    return 0 if res["test_acc"] >= 0.80 else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""OTA release signing — Ed25519 puro (RFC 8032), zero dependências.

Uso (dev machine; a seed privada NUNCA vai para o git):
  # 1) gerar par de chaves UMA vez (guarde a seed em local seguro):
  python tools/ota_sign.py --gen
  # 2) assinar o digest sha256 do kernel (serve_update.py faz isso sozinho):
  python tools/ota_sign.py --sign --key <seed-hex> --digest <sha256-hex>

O kernel pina a chave publica em identity.rs (TRUSTED_PUBLIC_KEYS) e rejeita
update sem `sig` valida no manifest (self_update.rs -> fetch_update).
Implementacao baseada na referencia do RFC 8032 — interopera com
ed25519_compact (Rust). `--selftest` valida contra o vetor 1 do RFC 8032.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys

P = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, P - 2, P)) % P
SQRT_M1 = pow(2, (P - 1) // 4, P)

# Base point (RFC 8032 / TweetNaCl): y = 4/5 (mod P), bit de sinal 0.
_G = bytes.fromhex("5866666666666666666666666666666666666666666666666666666666666666")


def _sha512(b: bytes) -> bytes:
    return hashlib.sha512(b).digest()


def _inv(x: int) -> int:
    return pow(x % P, P - 2, P)


def _xrecover(y: int) -> int:
    xx = (y * y - 1) * _inv(D * y * y + 1)
    x = pow(xx, (P + 3) // 8, P)
    if (x * x - xx) % P != 0:
        x = (x * SQRT_M1) % P
    if x % 2 != 0:
        x = P - x
    return x


def _point_add(p: tuple, q: tuple) -> tuple:
    (x1, y1), (x2, y2) = p, q
    x3 = (x1 * y2 + x2 * y1) * _inv(1 + D * x1 * x2 * y1 * y2)
    y3 = (y1 * y2 + x1 * x2) * _inv(1 - D * x1 * x2 * y1 * y2)
    return (x3 % P, y3 % P)


def _point_mul(s: int, p: tuple) -> tuple:
    q = (0, 1)
    while s > 0:
        if s & 1:
            q = _point_add(q, p)
        p = _point_add(p, p)
        s >>= 1
    return q


def _encodepoint(p: tuple) -> bytes:
    x, y = p
    return (y | ((x & 1) << 255)).to_bytes(32, "little")


def _decodepoint(s: bytes) -> tuple:
    n = int.from_bytes(s, "little")
    sign = n >> 255
    y = n & ((1 << 255) - 1)
    if y >= P:
        raise ValueError("bad y")
    x = _xrecover(y)
    if (x & 1) != sign:
        x = P - x
    return (x, y)


def _scalar_from_seed(seed: bytes) -> int:
    h = _sha512(seed)
    a = int.from_bytes(h[:32], "little")
    a &= (1 << 254) - 8
    a |= 1 << 254
    return a


def public_key(seed: bytes) -> bytes:
    a = _scalar_from_seed(seed)
    return _encodepoint(_point_mul(a, _decodepoint(_G)))


def sign(seed: bytes, msg: bytes) -> bytes:
    a = _scalar_from_seed(seed)
    prefix = _sha512(seed)[32:]
    A = public_key(seed)
    r = int.from_bytes(_sha512(prefix + msg), "little") % L
    R = _point_mul(r, _decodepoint(_G))
    k = int.from_bytes(_sha512(_encodepoint(R) + A + msg), "little") % L
    s = (r + k * a) % L
    return _encodepoint(R) + s.to_bytes(32, "little")


def verify(pk: bytes, msg: bytes, sig: bytes) -> bool:
    if len(pk) != 32 or len(sig) != 64:
        return False
    try:
        A = _decodepoint(pk)
        R = _decodepoint(sig[:32])
        s = int.from_bytes(sig[32:], "little")
        k = int.from_bytes(_sha512(sig[:32] + pk + msg), "little") % L
        return _point_mul(s, _decodepoint(_G)) == _point_add(R, _point_mul(k, A))
    except Exception:
        return False


# Vetor 1 do RFC 8032 (msg vazia) — prova interoperabilidade com ed25519_compact.
_RFC8032_TEST1_SEED = bytes.fromhex(
    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"
)
_RFC8032_TEST1_PK = bytes.fromhex(
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
)
_RFC8032_TEST1_SIG = bytes.fromhex(
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555"
    "fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
)


def selftest() -> bool:
    pk = public_key(_RFC8032_TEST1_SEED)
    sig = sign(_RFC8032_TEST1_SEED, b"")
    ok_pk = pk == _RFC8032_TEST1_PK
    ok_sig = sig == _RFC8032_TEST1_SIG
    ok_verify = verify(pk, b"", sig) and not verify(pk, b"tamper", sig)
    print(f"[selftest] pubkey ok={ok_pk} sig ok={ok_sig} verify ok={ok_verify}")
    return ok_pk and ok_sig and ok_verify


def main() -> int:
    ap = argparse.ArgumentParser(description="Ed25519 OTA release signing (RFC 8032)")
    ap.add_argument("--gen", action="store_true", help="gera seed + public key")
    ap.add_argument("--selftest", action="store_true", help="valida contra vetor RFC 8032")
    ap.add_argument("--sign", action="store_true", help="assina o digest (com --key + --digest)")
    ap.add_argument("--key", default=None, help="seed hex (32 bytes)")
    ap.add_argument("--digest", default=None, help="msg hex (ex. sha256 do kernel)")
    args = ap.parse_args()

    if args.selftest:
        return 0 if selftest() else 1

    if args.gen:
        seed = os.urandom(32)
        print(f"SEED={seed.hex()}")
        print(f"PUBKEY={public_key(seed).hex()}")
        print("GUARDE a SEED em local seguro (NUNCA commitar). Pina a PUBKEY no kernel.")
        return 0

    if not args.sign or not args.key or not args.digest:
        print("uso: --gen | --selftest | --sign --key <hex> --digest <hex>", file=sys.stderr)
        return 2

    seed = bytes.fromhex(args.key)
    if len(seed) != 32:
        print("seed deve ter 32 bytes (64 hex)", file=sys.stderr)
        return 2
    msg = bytes.fromhex(args.digest)
    sig = sign(seed, msg)
    print(f"SIG={sig.hex()}")
    print(f"PUBKEY={public_key(seed).hex()}")
    print(f"VERIFY={verify(public_key(seed), msg, sig)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

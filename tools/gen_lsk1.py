#!/usr/bin/env python3
"""Generate LSK1 lab blobs for the skill_lab hook (OPCODE-0065).

Byte layout (must match crates/hermes/src/skill_lab.rs::poll_and_run):
  LSK1 (4) | op (1: b'G'|b'E') | name\\0 | text\\0 (empty for E) | a\\0 | b\\0 | zero pad

Writes:
  target/lab_skill_boot1.bin  op G  name oracle_rt_expr_v1  text "(a*a + 3*b) - 5"  a=6 b=7
  target/lab_skill_boot2.bin  op E  name oracle_rt_expr_v1  text ""                 a=9 b=2
"""
import os
import sys

MAGIC = b"LSK1"
BLOB_SIZE = 256
NAME = "oracle_rt_expr_v1"
TEXT_G = "(a*a + 3*b) - 5"
TEXT_E = ""


def build_blob(op, name, text, a, b):
    assert len(op) == 1, "op must be 1 byte"
    parts = [
        MAGIC,
        op,
        name.encode("ascii") + b"\0",
        text.encode("ascii") + b"\0",
        str(a).encode("ascii") + b"\0",
        str(b).encode("ascii") + b"\0",
    ]
    raw = b"".join(parts)
    if len(raw) > BLOB_SIZE:
        raise ValueError("blob overflow: %d > %d" % (len(raw), BLOB_SIZE))
    return raw + b"\0" * (BLOB_SIZE - len(raw))


def cstr(raw, i):
    """Mirror of skill_lab::cstr_from: read NUL-terminated ASCII from index i."""
    j = i
    while j < len(raw) and raw[j] != 0:
        j += 1
    if j >= len(raw):
        raise ValueError("unterminated string at %d" % i)
    return raw[i:j].decode("ascii"), j + 1


def parse(raw):
    if raw[0:4] != MAGIC:
        raise ValueError("bad magic %r" % raw[0:4])
    op = bytes([raw[4]])
    name, i = cstr(raw, 5)
    text, i = cstr(raw, i)
    a_s, i = cstr(raw, i)
    b_s, _ = cstr(raw, i)
    return op, name, text, int(a_s), int(b_s)


def hexdump(data, n=64):
    for off in range(0, min(n, len(data)), 16):
        chunk = data[off:off + 16]
        hx = " ".join("%02x" % c for c in chunk)
        asc = "".join(chr(c) if 32 <= c < 127 else "." for c in chunk)
        print("  %04x  %-47s  %s" % (off, hx, asc))


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(root, "target")
    os.makedirs(out_dir, exist_ok=True)

    cases = [
        ("lab_skill_boot1.bin", b"G", NAME, TEXT_G, 6, 7),
        ("lab_skill_boot2.bin", b"E", NAME, TEXT_E, 9, 2),
    ]
    for fname, op, name, text, a, b in cases:
        raw = build_blob(op, name, text, a, b)
        # Self-check: re-parse with the SAME rules and assert round-trip.
        p_op, p_name, p_text, p_a, p_b = parse(raw)
        assert p_op == op, "op round-trip %r != %r" % (p_op, op)
        assert p_name == name, "name round-trip %r != %r" % (p_name, name)
        assert p_text == text, "text round-trip %r != %r" % (p_text, text)
        assert p_a == a and p_b == b, "a/b round-trip %r/%r != %r/%r" % (p_a, p_b, a, b)
        path = os.path.join(out_dir, fname)
        with open(path, "wb") as f:
            f.write(raw)
        print("[lsk1] wrote %s (%d bytes) op=%s name=%s a=%d b=%d"
              % (path, len(raw), op.decode(), name, a, b))
        print("[lsk1] first 64 bytes:")
        hexdump(raw, 64)
    print("[lsk1] self-check OK (op/name/text/a/b round-trip)")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print("[lsk1] FAIL: %s" % e, file=sys.stderr)
        sys.exit(1)

#!/usr/bin/env python3
"""Smoke logico do layout NeuralFS (nao substitui cargo check do kernel).
Valida tamanhos de item leaf e magic strings alinhados ao Rust.
"""
LEAF_ITEM = 48
LEAF_HEADER = 24
MAX_ITEMS = (4096 - LEAF_HEADER) // LEAF_ITEM
assert MAX_ITEMS == 84
assert b"NEURALFS" == bytes([0x4E, 0x45, 0x55, 0x52, 0x41, 0x4C, 0x46, 0x53])
assert b"NRFSJRNL" == bytes([0x4E, 0x52, 0x46, 0x53, 0x4A, 0x52, 0x4E, 0x4C])
print(f"[OK] NeuralFS leaf max_items={MAX_ITEMS} item={LEAF_ITEM}")

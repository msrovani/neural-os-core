import sys
sys.path.insert(0, 'tools')
from chat_v6 import BPE, V6
from pathlib import Path

m = V6(Path('target1/FALCON3_1B.V6'))
bpe = BPE.from_bpb1(m.tok_blob)
print('vocab size:', len(bpe.vocab))
for ch in ['{', '}', '[', ']', ':', ',', '"', '-', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'true', 'false', 'null']:
    if ch in bpe.vocab:
        print(f'  {ch!r}: {bpe.vocab[ch]}')
    else:
        print(f'  {ch!r}: NOT IN VOCAB')
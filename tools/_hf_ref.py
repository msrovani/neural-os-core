#!/usr/bin/env python3
"""HF reference forward — compara logits com dump do kernel."""
import json, struct, sys, os

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

model_name = '1bitLLM/bitnet_b1_58-xl'
prompt = 'ola'

print(f'[HF] Loading {model_name}...', flush=True)
tok = AutoTokenizer.from_pretrained(model_name, trust_remote_code=True)
print(f'[HF] Tokenizer OK. vocab_size={tok.vocab_size}', flush=True)

model = AutoModelForCausalLM.from_pretrained(
    model_name, trust_remote_code=True, torch_dtype=torch.float32,
)
model.eval()
print(f'[HF] Model loaded: {model.__class__.__name__}', flush=True)

inputs = tok(prompt, return_tensors='pt')
with torch.no_grad():
    outputs = model(input_ids=inputs['input_ids'], return_dict=True)

logits = outputs.logits[0, -1, :]
vals, ids = torch.topk(logits, 16)

result = {
    'model': model_name,
    'prompt': prompt,
    'input_ids': inputs['input_ids'][0].tolist(),
    'vocab_size': logits.shape[0],
    'ids': ids.tolist(),
    'logits': [round(v.item(), 6) for v in vals],
    'logits_bits': [struct.unpack('<I', struct.pack('<f', v.item()))[0] for v in vals],
}
print(json.dumps(result, indent=2, ensure_ascii=False))

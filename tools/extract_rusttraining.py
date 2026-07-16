#!/usr/bin/env python3
"""Extract Rust code blocks from Microsoft RustTraining mdbooks for RustCoder training.

Creates training pairs (section_context → rust_code) for all 7 books.
Output: tools/target/rusttraining_pairs.json
"""

import json, os, re, sys
from pathlib import Path

TRAINING_DIR = Path(r"C:\Users\msrov\AppData\Local\Temp\opencode\RustTraining")
OUTPUT = Path(__file__).parent / "target" / "rusttraining_pairs.json"

BOOKS = [
    "c-cpp-book",
    "csharp-book",
    "python-book",
    "async-book",
    "rust-patterns-book",
    "type-driven-correctness-book",
    "engineering-book",
]

def extract_pairs(book_dir: Path):
    """Extract (context, code) pairs from one book.
    
    For each ```rust block, collect the most recent heading + paragraph text
    before it as context.
    """
    src = book_dir / "src"
    if not src.exists():
        print(f"  [SKIP] {book_dir.name}: no src/")
        return []
    
    pairs = []
    md_files = sorted(src.glob("*.md"))
    for mf in md_files:
        content = mf.read_text(encoding="utf-8", errors="replace")
        lines = content.split("\n")
        
        current_heading = ""
        context_buf = []
        in_rust_block = False
        rust_lines = []
        heading_before_block = ""
        text_before_block = ""
        
        for i, line in enumerate(lines):
            heading_m = re.match(r'^#{1,4}\s+(.+)$', line)
            if heading_m and not line.startswith('###'):
                current_heading = heading_m.group(1).strip()
            
            if line.startswith("```rust"):
                in_rust_block = True
                rust_lines = []
                heading_before_block = current_heading
                # collect up to 5 preceding non-empty, non-heading lines as context
                ctx = []
                for j in range(min(5, len(context_buf))):
                    l = context_buf[-(j+1)]
                    if l.strip() and not l.startswith('#'):
                        ctx.insert(0, l.strip())
                text_before_block = " ".join(ctx[-3:]) if ctx else ""
                continue
            
            if in_rust_block:
                if line.startswith("```"):
                    in_rust_block = False
                    code = "\n".join(rust_lines).strip()
                    if len(code) >= 10:  # skip trivial snippets
                        ctx = heading_before_block
                        if text_before_block:
                            ctx = f"{ctx}: {text_before_block}"
                        pairs.append((ctx, code))
                else:
                    rust_lines.append(line)
            else:
                context_buf.append(line)
                if len(context_buf) > 100:
                    context_buf.pop(0)
    
    print(f"  {book_dir.name}: {len(pairs)} pairs from {len(md_files)} files")
    return pairs


def main():
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    
    all_pairs = []
    for book_name in BOOKS:
        book_dir = TRAINING_DIR / book_name
        if not book_dir.exists():
            print(f"  [SKIP] {book_name}: not found")
            continue
        pairs = extract_pairs(book_dir)
        all_pairs.extend(pairs)
    
    print(f"\nTotal: {len(all_pairs)} training pairs from {len(BOOKS)} books")
    
    # Deduplicate by code
    seen = set()
    deduped = []
    for ctx, code in all_pairs:
        if code not in seen:
            seen.add(code)
            deduped.append({"context": ctx, "code": code})
    
    print(f"Deduplicated: {len(deduped)} unique pairs (removed {len(all_pairs) - len(deduped)} duplicates)")
    
    with open(OUTPUT, "w", encoding="utf-8") as f:
        json.dump(deduped, f, ensure_ascii=False, indent=1)
    print(f"Saved: {OUTPUT} ({OUTPUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()

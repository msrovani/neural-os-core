from pathlib import Path

for p in Path("crates/k_hal/src").rglob("*.rs"):
    lines = p.read_text(encoding="utf-8").splitlines()
    changed = False
    out = []
    for line in lines:
        if "slog_hal!" in line and line.rstrip().endswith(";") and not line.rstrip().endswith(");"):
            # missing closing paren before semicolon
            if line.rstrip().endswith("len();") or line.count("(") > line.count(")"):
                line = line.rstrip()[:-1] + ");"
                changed = True
                print(f"fixed {p}: {line[-90:]}")
        out.append(line)
    if changed:
        p.write_text("\n".join(out) + "\n", encoding="utf-8")

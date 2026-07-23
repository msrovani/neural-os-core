from pathlib import Path

bad = []
for p in Path("crates/k_hal/src").rglob("*.rs"):
    text = p.read_text(encoding="utf-8")
    # crude: slog_hal! ... ; without ); before end if open parens
    i = 0
    while True:
        j = text.find("slog_hal!", i)
        if j < 0:
            break
        # find matching end - scan until we have balanced () then ;
        k = j + len("slog_hal!")
        depth = 0
        started = False
        end = None
        while k < len(text):
            c = text[k]
            if c == "(":
                depth += 1
                started = True
            elif c == ")":
                depth -= 1
                if started and depth == 0:
                    end = k
                    break
            k += 1
        if end is None:
            # find line number
            line = text[:j].count("\n") + 1
            bad.append((str(p), line, text[j : j + 120].replace("\n", " ")))
        i = j + 1

for b in bad:
    print(b)
print("count", len(bad))

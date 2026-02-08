import os, re, glob, sys

bad = []

for fp in glob.glob("docs/*.html"):
    with open(fp, "r", encoding="utf-8") as f:
        html = f.read()

    # Check href="..." and src="..."
    for attr in ("href", "src"):
        for h in re.findall(rf'{attr}="([^"]+)"', html):
            if h.startswith(("http", "mailto:", "#", "data:")):
                continue
            p = os.path.join("docs", h)
            if not os.path.exists(p):
                bad.append((fp, f"{attr}={h}"))

if bad:
    print("❌ Missing link targets:")
    for fp, item in bad:
        print(f"  {fp} -> {item}")
    sys.exit(2)

print("✅ All internal href/src targets exist")

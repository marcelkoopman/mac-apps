#!/usr/bin/env bash
# Count real `unsafe` in apps/copycraft and crates/mac-ui: blocks, `unsafe fn` and `unsafe impl`,
# separately per crate. Comments, string/char literals and `#[unsafe(...)]` attributes (for
# example `#[unsafe(method(...))]` in define_class!) are ignored.
#
#   scripts/count_unsafe.sh            print the counts (the format of scripts/unsafe_baseline.txt)
#   scripts/count_unsafe.sh --check    fail when a count is above scripts/unsafe_baseline.txt
set -euo pipefail
cd "$(dirname "$0")/.."

counts() {
python3 - <<'PY'
import os, re

CRATES = [("copycraft", "apps/copycraft"), ("mac-ui", "crates/mac-ui")]

def strip(src):
    """Source with comments and string/char literals blanked out."""
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            out.append(" ")
        elif (m := re.match(r'(?:b|c)?r(#*)"', src[i:])) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            end = '"' + m.group(1)
            j = src.find(end, i + m.end())
            i = n if j < 0 else j + len(end)
            out.append('""')
        elif c == '"' or (c in "bc" and src.startswith('"', i + 1) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_"))):
            i = src.index('"', i) + 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            i += 1
            out.append('""')
        elif c == "'" and (m := re.match(r"'(\\.[^']*|[^\\'])'", src[i:])):
            i += m.end()
            out.append("' '")
        else:
            out.append(c)
            i += 1
    return "".join(out)

for name, root in CRATES:
    total = {"blocks": 0, "fn": 0, "impl": 0}
    for dirpath, dirs, files in os.walk(root):
        dirs[:] = sorted(d for d in dirs if d != "target")
        for f in sorted(files):
            if not f.endswith(".rs"):
                continue
            text = strip(open(os.path.join(dirpath, f), encoding="utf-8").read())
            for m in re.finditer(r"\bunsafe\b\s*(\S)", text):
                rest = text[m.start(1):]
                if m.group(1) == "{":
                    total["blocks"] += 1
                elif re.match(r"(extern\s*(\"\")?\s*)?fn\b", rest):
                    total["fn"] += 1
                elif rest.startswith("impl"):
                    total["impl"] += 1
                # `#[unsafe(...)]` attributes and `unsafe extern {}` declarations are not counted.
    for kind, value in total.items():
        print(f"{name} {kind} {value}")
PY
}

if [[ "${1:-}" == "--check" ]]; then
    current="$(counts)"
    failed=0
    while read -r crate kind limit; do
        [[ -z "$crate" || "$crate" == \#* ]] && continue
        now="$(awk -v c="$crate" -v k="$kind" '$1 == c && $2 == k { print $3 }' <<<"$current")"
        if (( now > limit )); then
            echo "error: $crate has $now unsafe $kind, above the baseline of $limit in scripts/unsafe_baseline.txt." >&2
            echo "       Move the new unsafe into a safe mac-ui helper, or justify it and raise the baseline." >&2
            failed=1
        fi
    done < scripts/unsafe_baseline.txt
    echo "$current"
    exit "$failed"
fi
counts

#!/bin/sh
# Writes the large Copycraft test inputs that are not committed:
#   large_1mb.csv        about 1 MB, a CSV table (large-table preview and steps)
#   large_over_8mb.txt   8,000,067 bytes of text: just over Copycraft's 8 MB file limit
#                        (open_file::MAX_FILE_BYTES = 8,000,000), so dropping or opening it
#                        shows "File is larger than 8 MB"
# Usage: sh apps/copycraft/testdata/generate_large.sh [output-dir]
# Default output dir: target/copycraft-testdata (relative to the current directory).
set -eu
out="${1:-target/copycraft-testdata}"
mkdir -p "$out"

csv="$out/large_1mb.csv"
echo "id,date,meter,usage_kwh,cost_eur" > "$csv"
# About 33 bytes per row: 32,000 rows is a little over 1 MB (1,064,568 bytes).
awk 'BEGIN {
  for (i = 1; i <= 32000; i++) {
    printf "%d,2026-%02d-%02d,M-%02d,%d.%d,%d.%02d\n", i, (i % 12) + 1, (i % 28) + 1, i % 50, i % 400, i % 10, i % 90, i % 100
  }
}' >> "$csv"

txt="$out/large_over_8mb.txt"
# 71 bytes per line; 112,677 lines = 8,000,067 bytes, just over 8,000,000.
awk 'BEGIN {
  line = "Synthetic Copycraft filler text, nothing sensitive in here at all.....\n";
  for (i = 0; i < 112677; i++) printf "%s", line
}' > "$txt"

ls -l "$csv" "$txt"

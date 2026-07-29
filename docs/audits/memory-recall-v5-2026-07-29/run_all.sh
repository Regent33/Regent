#!/bin/bash
# Full v5 matrix: 2 arms x 2 corpora x 2 systems x 12 seeds = 96 runs.
# Emits rankings and rendered text only. Metrics come from score.py, once.
#
#   ./run_all.sh <hermes-src> <runs-dir>
set -u
HERMES_SRC="$1"
RUNS="$2"
HERE="$(cd "$(dirname "$0")" && pwd)"
BENCH="$HERE/harness/recallbench/target/release/recallbench.exe"
mkdir -p "$RUNS"
fail=0

for arm in shipped raised; do
  for tag in base intervention; do
    [ "$tag" = base ] && corpus=corpus.json || corpus=corpus-intervention.json
    for seed in 101 102 103 104 105 106 107 108 109 110 111 112; do
      out_r="$RUNS/regent-$arm-$tag-s$seed.json"
      out_h="$RUNS/hermes-$arm-$tag-s$seed.json"
      "$BENCH" "$HERE" "$seed" "$arm" "$corpus" "$out_r" >/dev/null 2>>"$RUNS/errors.log" \
        || { echo "FAIL regent $arm $tag s$seed"; fail=1; }
      PYTHONIOENCODING=utf-8 python "$HERE/harness/hermes_run.py" "$HERMES_SRC" "$HERE" \
        "$seed" "$arm" "$corpus" "$out_h" >/dev/null 2>>"$RUNS/errors.log" \
        || { echo "FAIL hermes $arm $tag s$seed"; fail=1; }
    done
    echo "done $arm/$tag"
  done
done

echo "runs written: $(ls "$RUNS"/*.json 2>/dev/null | wc -l) (expect 96), failures: $fail"
exit $fail

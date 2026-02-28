#!/usr/bin/env bash
set -euo pipefail

# Scaffold benchmark for collecting sidecar embedding baseline metrics.
# Usage:
#   scripts/benchmarks/embedding_sidecar_baseline.sh [iterations]

iterations="${1:-3}"

if ! [[ "${iterations}" =~ ^[0-9]+$ ]] || [[ "${iterations}" -lt 1 ]]; then
  echo "iterations must be a positive integer" >&2
  exit 1
fi

resolve_python() {
  if [[ -n "${JULIE_TEST_PYTHON:-}" ]]; then
    echo "${JULIE_TEST_PYTHON}"
    return 0
  fi

  local candidates
  if [[ "$(uname -s)" == "MINGW"* ]] || [[ "$(uname -s)" == "MSYS"* ]] || [[ "$(uname -s)" == "CYGWIN"* ]]; then
    candidates=(python py python3)
  else
    candidates=(python3 python)
  fi

  local candidate
  for candidate in "${candidates[@]}"; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      echo "${candidate}"
      return 0
    fi
  done

  return 1
}

python_bin="$(resolve_python || true)"
if [[ -z "${python_bin}" ]]; then
  echo "No Python interpreter found. Set JULIE_TEST_PYTHON to override." >&2
  exit 1
fi

echo "Running sidecar baseline scaffold (${iterations} iterations)"
echo "Python: ${python_bin}"
echo "Provider: sidecar"
echo

total_ms=0
test_name="tests::integration::embedding_pipeline::tests::test_pipeline_embeds_with_sidecar_provider"
i=1
while (( i <= iterations )); do
  start_ns="$(date +%s%N)"
  test_output="$({
    JULIE_EMBEDDING_PROVIDER=sidecar \
    JULIE_EMBEDDING_SIDECAR_PROGRAM="${python_bin}" \
    cargo test --lib embedding_pipeline::tests::test_pipeline_embeds_with_sidecar_provider --features embeddings-sidecar
  } 2>&1)"
  end_ns="$(date +%s%N)"

  if [[ "${test_output}" == *"running 0 tests"* ]]; then
    echo "benchmark failed: matched test count was zero" >&2
    echo "${test_output}" >&2
    exit 1
  fi

  if [[ "${test_output}" != *"test ${test_name} ... ok"* ]]; then
    echo "benchmark failed: sidecar pipeline test did not run successfully" >&2
    echo "${test_output}" >&2
    exit 1
  fi

  elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
  total_ms=$(( total_ms + elapsed_ms ))
  echo "iteration ${i}: ${elapsed_ms} ms"
  ((i++))
done

avg_ms=$(( total_ms / iterations ))

cat <<EOF

Baseline scaffold complete.

- average test runtime: ${avg_ms} ms
- next step: replace this harness with a corpus-level embedding pass and collect
  symbols/sec from pipeline stats logs for true throughput tracking.
EOF

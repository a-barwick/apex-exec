#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

apex_exec_bin="${APEX_EXEC_BIN:-${repo_root}/target/release/apex-exec}"
artifact_dir="${CI_ARTIFACT_DIR:-${repo_root}/artifacts/apex-regression}"
mkdir -p "${artifact_dir}"

if [[ ! -x "${apex_exec_bin}" ]]; then
  echo "apex-exec binary is not executable: ${apex_exec_bin}" >&2
  exit 1
fi

case_count=0

run_case() {
  local label="$1"
  local expected="$2"
  shift 2

  local output
  local log_file="${artifact_dir}/${label}.log"
  if ! output="$("${apex_exec_bin}" "$@" 2>&1)"; then
    printf '%s\n' "${output}" | tee "${log_file}" >&2
    echo "Apex regression case failed: ${label}" >&2
    return 1
  fi
  printf '%s\n' "${output}" > "${log_file}"

  if [[ "${output}" != *"${expected}"* ]]; then
    printf '%s\n' "${output}" >&2
    echo "Apex regression case ${label} did not contain the expected result:" >&2
    printf '%s\n' "${expected}" >&2
    return 1
  fi

  case_count=$((case_count + 1))
  echo "PASS ${label}"
}

# Full anonymous programs cover nested control flow, collections, methods,
# overloads, recursion, casts, typed exceptions, and source-mapped execution.
run_case \
  "anonymous-billing" \
  $'billableLines=5\nsubtotal=2250\nstatus=priority' \
  run tests/scenarios/billing_summary.apex
run_case \
  "anonymous-collections" \
  "01234567891011121314151617181920" \
  run tests/scenarios/collections.apex
run_case \
  "anonymous-methods-exceptions" \
  "division finished" \
  run examples/methods-exceptions.apex

# Project fixtures deliberately grow in complexity. Together they exercise
# multi-file resolution, isolated Apex tests, metadata-backed SObjects,
# SOQL/SOSL/DML, transaction-aware triggers, platform APIs, and deterministic
# queueable/future/batch/scheduled/event work.
run_case \
  "m5-project-check" \
  "OK (3 classes, 3 source files)" \
  check examples/milestone5-project
run_case \
  "m5-project-invoke" \
  "Hello, Apex!" \
  invoke examples/milestone5-project Entry.run
run_case \
  "m6-isolated-tests" \
  "Summary: 2 passed, 0 failed, 2 total" \
  test examples/milestone6-project --jobs 2
run_case \
  "m7-schema-sobject" \
  $'Approved\n125' \
  invoke examples/milestone7-project InvoiceDemo.run
run_case \
  "m8-query-dml" \
  $'INV-100\nAcme\n1' \
  invoke examples/milestone8-project InvoiceDemo.run
run_case \
  "m9-trigger-transaction" \
  $'Increased\nRestored\n1' \
  invoke examples/milestone9-project TriggerDemo.run
run_case \
  "m9-trigger-tests" \
  "Summary: 3 passed, 0 failed, 3 total" \
  test examples/milestone9-project --jobs 2
run_case \
  "m10-platform-profile" \
  "12.25 | bWlsZXN0b25lLTEw | 10 | true | BYg" \
  invoke examples/milestone10-project PlatformDemo.run
run_case \
  "m10-platform-tests" \
  "Summary: 4 passed, 0 failed, 4 total" \
  test examples/milestone10-project --jobs 2
run_case \
  "m11-async-tests" \
  "Summary: 2 passed, 0 failed, 2 total" \
  test examples/milestone11-project --jobs 2
run_case \
  "m13-oracle-project-check" \
  "OK (" \
  check examples/milestone13-oracle/project
run_case \
  "m13-oracle-apex-test" \
  "Summary: 1 passed, 0 failed, 1 total" \
  test examples/milestone13-oracle/project OracleDemoTest.confirmsBehavior

echo "Apex regression suite passed ${case_count} end-to-end cases."

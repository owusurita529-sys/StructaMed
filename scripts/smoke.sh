#!/usr/bin/env bash
set -euo pipefail

echo "== StructaMed smoke test =="

echo "== format check =="
cargo fmt --check

echo "== unit/integration tests =="
cargo test --all

echo "== build release binary =="
cargo build --release
BIN=./target/release/clinote

echo "== CLI help =="
$BIN --help >/dev/null

echo "== docs hygiene (filenames) =="
if find docs -maxdepth 1 -type f -name '* *' | grep -q .; then
  echo "ERROR: docs/ contains filenames with spaces:"
  find docs -maxdepth 1 -type f -name '* *' -print
  exit 2
fi

echo "== docs hygiene (corruption markers) =="
if grep -RInE 'HTMLml>ipt|PYint|inputinput|smoke\.shun|docs\.htmlxtures|git diff' docs/; then
  echo "ERROR: found corruption markers in docs/"
  exit 2
fi

echo "== docs internal links/assets =="
python3 scripts/check_docs_links.py

echo "== strict clean fixtures (must pass) =="
$BIN selftest --fixtures tests/fixtures/clean/soap --template soap --strict
$BIN selftest --fixtures tests/fixtures/clean/hp --template hp --strict
$BIN selftest --fixtures tests/fixtures/clean/discharge --template discharge --strict

echo "== messy fixtures (allowed to warn, must not crash) =="
set +e
$BIN selftest --fixtures tests/fixtures --template soap
CODE=$?
set -e
echo "(non-strict messy run exit=$CODE)"

echo "OK ✅"

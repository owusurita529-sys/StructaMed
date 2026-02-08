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

echo "== docs internal links/assets =="
python3 scripts/check_docs_links.py

$BIN --help >/dev/null

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

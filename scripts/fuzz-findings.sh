#!/usr/bin/env bash
# Fuzz findings are security issues (SECURITY.md), and the logs and
# artifacts of a public repository's workflow runs are public, so the
# nightly fuzz workflow seals what it finds to the maintainers' key
# (.github/fuzz-findings.pub.pem) and a maintainer opens it with the
# private key.
#
#   scripts/fuzz-findings.sh seal DIR       # fuzz/artifacts and fuzz/logs → DIR/{key,findings}.enc
#   scripts/fuzz-findings.sh open RUN_ID    # downloads a run's sealed findings into fuzz/findings/RUN_ID
#
# The private key is read from INKOXIDE_FUZZ_KEY, by default
# ~/.config/inkoxide/fuzz-findings.key.pem.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"

# A random password encrypts the findings (AES-256); the public key
# encrypts the password (RSA-OAEP).
seal() {
  local out=$1 tmp
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN
  mkdir -p "$out"
  tar -czf "$tmp/findings.tgz" -C "$root/fuzz" artifacts logs
  openssl rand -hex 32 >"$tmp/password"
  openssl enc -aes-256-cbc -pbkdf2 -iter 200000 -salt -pass "file:$tmp/password" \
    -in "$tmp/findings.tgz" -out "$out/findings.enc"
  openssl pkeyutl -encrypt -pubin -inkey "$root/.github/fuzz-findings.pub.pem" \
    -pkeyopt rsa_padding_mode:oaep -pkeyopt rsa_oaep_md:sha256 \
    -in "$tmp/password" -out "$out/key.enc"
}

# Decrypts DIR/{key,findings}.enc into DIR/findings.
unseal() {
  local dir=$1 key=${INKOXIDE_FUZZ_KEY:-$HOME/.config/inkoxide/fuzz-findings.key.pem} tmp
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN
  openssl pkeyutl -decrypt -inkey "$key" \
    -pkeyopt rsa_padding_mode:oaep -pkeyopt rsa_oaep_md:sha256 \
    -in "$dir/key.enc" -out "$tmp/password"
  openssl enc -d -aes-256-cbc -pbkdf2 -iter 200000 -pass "file:$tmp/password" \
    -in "$dir/findings.enc" -out "$tmp/findings.tgz"
  mkdir -p "$dir/findings"
  tar -xzf "$tmp/findings.tgz" -C "$dir/findings"
}

case "${1:-}" in
  seal)
    seal "${2:?usage: fuzz-findings.sh seal DIR}" ;;
  unseal)
    unseal "${2:?usage: fuzz-findings.sh unseal DIR}" ;;
  open)
    run="${2:?usage: fuzz-findings.sh open RUN_ID}"
    dest="$root/fuzz/findings/$run"
    repo=$(node "$root/scripts/release/common.mjs" repository-slug)
    gh run download "$run" --repo "$repo" --pattern 'fuzz-findings-*' --dir "$dest"
    for dir in "$dest"/fuzz-findings-*; do
      unseal "$dir"
      echo "${dir#"$root/"}/findings: $(find "$dir/findings/artifacts" -type f | wc -l | tr -d ' ') inputs"
    done
    echo "reproduce one with: cd fuzz && cargo run --release --bin repro -- <tree|json|image|svg|font> <input>"
    ;;
  *)
    echo "usage: fuzz-findings.sh seal DIR | unseal DIR | open RUN_ID" >&2
    exit 2 ;;
esac

#!/usr/bin/env bash
# Runs a command that downloads something, up to three times. Downloads from GitHub's releases and
# from the Rust toolchain server fail now and then for a moment (a TLS handshake EOF, a refused
# HTTP/2 stream, a 504), and failed the whole job, and with it E2E and Release compatibility.
# Only for downloads, which are safe to repeat; builds and tests run once. Each failed attempt is
# a warning in the run's annotations, so the flakes stay visible. After the last one the step
# fails with the command's exit code.
#
#   .github/scripts/retry.sh rustup toolchain install
#
# RETRY_ATTEMPTS and RETRY_PAUSE (seconds before the second attempt, tripled for each later one)
# change the defaults of 3 and 10. Bash 3.2 (macOS) runs it too.
set -u

if [ "$#" -eq 0 ]; then
  echo "usage: $0 command [argument...]" >&2
  exit 2
fi

attempts=${RETRY_ATTEMPTS:-3}
pause=${RETRY_PAUSE:-10}
attempt=1
while :; do
  "$@"
  status=$?
  if [ "$status" -eq 0 ]; then
    exit 0
  fi
  if [ "$attempt" -ge "$attempts" ]; then
    echo "::error::$* failed $attempts times, the last time with exit code $status"
    exit "$status"
  fi
  echo "::warning::$* failed with exit code $status (attempt $attempt of $attempts), again in ${pause}s"
  sleep "$pause"
  pause=$((pause * 3))
  attempt=$((attempt + 1))
done

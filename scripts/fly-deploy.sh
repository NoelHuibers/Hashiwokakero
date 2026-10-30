#!/usr/bin/env bash
# `flyctl deploy` with its arguments, tried up to three times: Fly's API answers 503 now and
# then for a minute (twice on 2026-10-29, once creating the release, once looking up the
# organization), and a deploy that fails for it would otherwise need the whole job rerun.
# Deploying the same image again is harmless, so any failure is tried again, after a pause.
# flyctl's last line (the release, or what went wrong) goes to the job summary.
#
#   scripts/fly-deploy.sh --local-only --dockerfile Dockerfile.prebuilt
#
# FLY_DEPLOY_ATTEMPTS and FLY_DEPLOY_PAUSE (seconds, doubled after each failure) change the
# defaults of 3 and 30.
set -uo pipefail

attempts=${FLY_DEPLOY_ATTEMPTS:-3}
pause=${FLY_DEPLOY_PAUSE:-30}
summary=${GITHUB_STEP_SUMMARY:-/dev/null}

for ((attempt = 1; ; attempt++)); do
  if flyctl deploy "$@" | tail -n 1 >> "$summary"; then
    exit 0
  fi
  if ((attempt >= attempts)); then
    echo "::error::flyctl deploy failed $attempts times"
    exit 1
  fi
  echo "::warning::flyctl deploy failed (attempt $attempt of $attempts), again in ${pause}s"
  sleep "$pause"
  pause=$((pause * 2))
done

#!/usr/bin/env bash
# The image the deploy ships (Dockerfile.prebuilt), built from the CI's binary and site in dist/,
# started like on Fly: it has to serve the home page with everything that hydrates it. CI runs
# it for every pull request, so an update of the base image is tested before it deploys.
#
#   scripts/docker-smoke.sh                 with docker
#   DOCKER=podman scripts/docker-smoke.sh   with podman
#
# dist/ holds hashi, hash.txt and site/, see the "Package" steps of the build jobs.
set -euo pipefail

engine=${DOCKER:-docker}
image=hashi-smoke
port=${SMOKE_PORT:-8080}
url=http://127.0.0.1:$port

$engine build --file Dockerfile.prebuilt --tag $image .
# Published test-only seeds: production startup enforces explicit configuration.
container=$($engine run --detach --publish 127.0.0.1:$port:8080 \
  --env HASHI_DAILY_SECRET=smoke-test-only-daily-secret-00000000 \
  --env DOJO_SECRET=smoke-test-only-dojo-secret-000000000 $image)
finish() {
  status=$?
  if [ $status -ne 0 ]; then
    echo "--- the container's log ---"
    $engine logs "$container" 2>&1 | tail -n 50
  fi
  $engine rm --force "$container" >/dev/null
  exit $status
}
trap finish EXIT

# Migrating the fresh database takes a moment; at most 30 s
for _ in $(seq 60); do
  curl --silent --fail --output /dev/null "$url/favicon.svg" && break
  sleep 0.5
done

# Fails on any status but 200, prints the content type
fetch() {
  curl --silent --show-error --fail --output /dev/null --write-out '%{content_type}' "$url$1"
}
expect_type() {
  type=$(fetch "$1")
  case $type in
    "$2"*) echo "ok $1 ($type)" ;;
    *) echo "$1 is $type, not $2" >&2; exit 1 ;;
  esac
}

html=$(curl --silent --show-error --fail "$url/")
grep -q '<main' <<<"$html" || { echo "/ has no page in it" >&2; exit 1; }
echo "ok / (200)"
# What hydrates the page, with the hashed names of this build, and a font
for asset in "/pkg/hashi[^\"']*\.js" "/pkg/hashi[^\"']*\.wasm" "/pkg/hashi[^\"']*\.css" "/fonts/[^\"']*\.css"; do
  path=$(grep -o "$asset" <<<"$html" | head -n 1)
  [ -n "$path" ] || { echo "/ doesn't link $asset" >&2; exit 1; }
  case $path in
    *.wasm) expect_type "$path" application/wasm ;;
    *.js) expect_type "$path" text/javascript ;;
    *.css) expect_type "$path" text/css ;;
  esac
done
# A page that needs the database
expect_type /ranks text/html
# A social preview image, drawn by the server with the fonts compiled into it
expect_type /og/ja.png image/png

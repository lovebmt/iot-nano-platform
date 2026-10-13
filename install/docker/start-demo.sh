#!/usr/bin/env bash
# Starts a fresh local Docker deployment and loads the default Power Monitor demo.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
ready_url="http://127.0.0.1:17180/readyz"
clean=0

usage() {
  cat <<'EOF'
Usage: bash start-demo.sh [--clean]

Options:
  --clean   Stop the Docker stack and delete the existing iot-nano-data volume
            before bootstrapping and seeding the demo. This permanently removes
            the prior platform data.
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --clean) clean=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'start-demo: unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

for required_command in docker curl jq; do
  command -v "$required_command" >/dev/null 2>&1 || {
    printf 'start-demo: required command is unavailable: %s\n' "$required_command" >&2
    exit 1
  }
done

cd "$script_dir"

if [[ ! -f .env ]]; then
  cp .env.example .env
fi

if [[ "$clean" -eq 1 ]]; then
  printf '%s\n' 'start-demo: removing the existing IoT Nano Docker data volume.'
  docker compose down -v --remove-orphans
fi

docker compose build

# These credentials are deliberately limited to a disposable local demo.
export IOT_NANO_BOOTSTRAP_SYSTEM_USERNAME=systemadmin
export IOT_NANO_BOOTSTRAP_SYSTEM_PASSWORD=systemadmin
export IOT_NANO_ALLOW_INSECURE_DEFAULT_PASSWORDS=true
if ! docker compose run --rm \
  -e IOT_NANO_BOOTSTRAP_SYSTEM_USERNAME \
  -e IOT_NANO_BOOTSTRAP_SYSTEM_PASSWORD \
  iot-nano --bootstrap-system; then
  printf '%s\n' 'start-demo: bootstrap failed; run again with --clean to replace the existing Docker data volume.' >&2
  exit 1
fi
unset IOT_NANO_BOOTSTRAP_SYSTEM_USERNAME IOT_NANO_BOOTSTRAP_SYSTEM_PASSWORD

docker compose up -d

for attempt in $(seq 1 60); do
  if curl --fail --silent "$ready_url" >/dev/null; then
    "$repo_root/scripts/install-linux-monolith.sh" seed-demo
    printf '%s\n' 'Power Monitor demo seed completed.'
    printf '%s\n' 'IoT Nano: http://127.0.0.1:17180'
    printf '%s\n' 'Demo users: user1/user1 (owner), user2/user2 (viewer)'
    exit 0
  fi
  sleep 1
done

printf 'start-demo: service did not become ready at %s\n' "$ready_url" >&2
docker compose logs --tail=100 iot-nano >&2 || true
exit 1

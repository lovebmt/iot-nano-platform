#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="$root/install/docker/compose.yaml"
env_example="$root/install/docker/.env.example"
dockerfile="$root/install/docker/Dockerfile"
entrypoint="$root/install/docker/entrypoint.sh"
demo_launcher="$root/install/docker/start-demo.sh"

fail() {
  printf 'test-docker-install: %s\n' "$*" >&2
  exit 1
}

assert_contains() {
  local text="$1"
  local expected="$2"

  [[ "$text" == *"$expected"* ]] || fail "expected output to contain $expected"
}

[[ -f "$compose_file" ]] || fail 'compose file must exist'
[[ -f "$env_example" ]] || fail '.env.example must exist'
[[ -f "$dockerfile" ]] || fail 'Dockerfile must exist'
[[ -f "$entrypoint" ]] || fail 'entrypoint must exist'
[[ -f "$demo_launcher" ]] || fail 'demo launcher must exist'

compose="$(<"$compose_file")"
env="$(<"$env_example")"
dockerfile_contents="$(<"$dockerfile")"
entrypoint_contents="$(<"$entrypoint")"
demo_launcher_contents="$(<"$demo_launcher")"
assert_contains "$compose" '"${HTTP_PORT:-17180}:18080"'
assert_contains "$compose" '"${MQTT_PORT:-17183}:1883"'
assert_contains "$env" 'HTTP_PORT=17180'
assert_contains "$env" 'MQTT_PORT=17183'
assert_contains "$compose" 'IOT_NANO_MQTT_TLS_ENABLED: "${IOT_NANO_MQTT_TLS_ENABLED:-false}"'
assert_contains "$env" 'IOT_NANO_MQTT_TLS_ENABLED=false'
[[ "$dockerfile_contents" != *' + &&'* ]] || fail 'Dockerfile must use valid shell command chains'
assert_contains "$dockerfile_contents" 'COPY Cargo.toml Cargo.lock ./'
assert_contains "$dockerfile_contents" 'COPY crates crates'
assert_contains "$dockerfile_contents" 'COPY services services'
assert_contains "$dockerfile_contents" 'RUN cargo build --release --locked --package iot-nano-monolith'
[[ "$dockerfile_contents" != *'COPY . .'* ]] || fail 'Dockerfile must not invalidate the Rust build cache for installer-only changes'
assert_contains "$dockerfile_contents" 'mkdir -p /var/lib/iot-nano/platform /var/lib/iot-nano/tls'
[[ "$dockerfile_contents" != *'mkdir -p /var/lib/iot-nano/platform /var/lib/iot-nano/internal /var/lib/iot-nano/tls'* ]] || fail 'Dockerfile must not pre-create the runtime-owned internal state directory'
assert_contains "$entrypoint_contents" 'mkdir -p "$state_root/platform" "$tls_root"'
[[ "$entrypoint_contents" != *'mkdir -p "$state_root/platform" "$state_root/internal" "$tls_root"'* ]] || fail 'entrypoint must leave the internal state directory for the runtime to create securely'
assert_contains "$entrypoint_contents" 'exec /usr/local/bin/iot-nano-monolith "$@"'
assert_contains "$demo_launcher_contents" '--clean'
assert_contains "$demo_launcher_contents" 'docker compose down -v'

printf 'test-docker-install: ok\n'

# Single-container Docker install

From this directory, start IoT Nano with one container and one persistent
Docker volume:

```sh
cp .env.example .env
docker compose up -d --build
```

The first build on a new machine compiles the Rust service locally. Subsequent
changes limited to Docker configuration or this documentation reuse the Rust
build layer.

To replace all existing Docker platform data and start the seeded Power Monitor
demo, run:

```sh
bash start-demo.sh --clean
```

`--clean` deletes the `iot-nano-data` volume before bootstrapping the demo.

Verify it:

```sh
curl http://127.0.0.1:17180/healthz
curl http://127.0.0.1:17180/readyz
```

The ports are configured in `.env`; the supplied defaults are HTTP `17180`
and MQTT `17183`. MQTT TLS is disabled by default. To expose it, set
`IOT_NANO_MQTT_TLS_ENABLED=true` and add a `8883` port mapping in your local
Compose override.

The container owns SQLite state, internal service state, a generated device
token vault key, and an MQTT TLS self-signed certificate in the
`iot-nano-data` volume. Do not remove that volume unless intentionally
resetting the platform:

```sh
docker compose down -v
```

HTTP runs without browser HTTPS by default. When MQTT TLS is enabled, it uses
the generated self-signed certificate.

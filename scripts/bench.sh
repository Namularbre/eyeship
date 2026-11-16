#!/usr/bin/env bash
# ENF1 benchmark: audit a 50-service Compose file, report wall time and peak RSS.
# Budget: < 100 ms and < 20 MB. Usage: scripts/bench.sh [SERVICES]
set -euo pipefail

services="${1:-50}"
root="$(cd "$(dirname "$0")/.." && pwd)"
bin="$root/target/release/eyeship"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo build --release --quiet --manifest-path "$root/Cargo.toml" -p eyeship-cli

{
  echo "name: bench"
  echo "services:"
  for i in $(seq 0 $((services - 1))); do
    n=$(printf '%02d' "$i")
    cat <<YAML
  svc-$n:
    image: registry.example.com/team/svc-$n:1.$i.0
    restart: unless-stopped
YAML
    if [ "$i" -gt 0 ]; then
      printf '    depends_on:\n      svc-%02d:\n        condition: service_healthy\n' $((i - 1))
    fi
    cat <<YAML
    environment:
      LOG_LEVEL: \${LOG_LEVEL:-info}
      SERVICE_NAME: svc-$n
    ports:
      - "$((10000 + i)):8080"
    volumes:
      - data-$n:/var/lib/app
    networks: [backend]
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 3s
      retries: 5
    logging:
      driver: json-file
      options:
        max-size: 10m
        max-file: "3"
    deploy:
      resources:
        limits:
          memory: 256M
YAML
  done
  printf 'networks:\n  backend: {}\nvolumes:\n'
  for i in $(seq 0 $((services - 1))); do printf '  data-%02d: {}\n' "$i"; done
} > "$work/compose.yaml"

echo "file: $services services, $(wc -l < "$work/compose.yaml") lines, $(wc -c < "$work/compose.yaml") bytes"
"$bin" compose "$work/compose.yaml" > /dev/null || true # warm-up

if [ ! -x /usr/bin/time ]; then
  echo "GNU time is required for the peak RSS (apt-get install time / brew install gnu-time)" >&2
  exit 2
fi

/usr/bin/time -v "$bin" compose "$work/compose.yaml" 2> "$work/time.txt" > /dev/null || true
grep -E 'Elapsed \(wall clock\)|Maximum resident set size' "$work/time.txt" | sed 's/^[[:space:]]*//'
rss_kb=$(sed -n 's/^[[:space:]]*Maximum resident set size (kbytes): //p' "$work/time.txt")

best=""
for _ in $(seq 10); do
  start=$(date +%s%N)
  "$bin" compose "$work/compose.yaml" > /dev/null || true
  elapsed=$(( ($(date +%s%N) - start) / 1000 ))
  if [ -z "$best" ] || [ "$elapsed" -lt "$best" ]; then best=$elapsed; fi
done
printf 'Wall time, best of 10 (ms): %d.%03d\n' $((best / 1000)) $((best % 1000))
echo "Budget (ENF1, 50 services): < 100 ms, < 20 MB (20480 kB)"

# Exit 1 when the budget is exceeded, so CI fails (spec 003, T003).
status=0
if [ "$best" -ge 100000 ]; then echo "FAIL: wall time over budget" >&2; status=1; fi
if [ -z "$rss_kb" ] || [ "$rss_kb" -ge 20480 ]; then echo "FAIL: peak RSS over budget (${rss_kb:-unknown} kB)" >&2; status=1; fi
exit "$status"

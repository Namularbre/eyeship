# eyeship

> *eyeship*, pronounced "I ship": the watching eye (*eye*), the vessel (*ship*), and "I ship to production" (*I ship*).

`eyeship` audits your Docker Compose files and finds, **before you go to production**, what is likely to bring your application down: a dependency started before the database is ready, a container without a memory limit, logs filling up the disk, a port published twice, an undefined variable…

- **Static, offline** analysis: no container started, no Docker daemon, no network.
- Every finding says **where** (file, service, line), **why**, and **how to fix it**.
- Built for **CI**: stable exit codes, JSON output, configurable failure threshold, deterministic results.
- A single static binary (a few MB), fast: 50 services audited in ~6 ms with ~5 MB of RAM.

## Installation

**Binaries**: download the archive for your platform (Linux musl x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64) from the [GitHub releases](https://github.com/Namularbre/eyeship/releases).

**From source** (Rust ≥ 1.85):

```sh
cargo install --git https://github.com/Namularbre/eyeship eyeship-cli
```

**Docker** (a `scratch` image of a few MB):

```sh
docker run --rm -v "$PWD:/work" ghcr.io/namularbre/eyeship compose
```

## Usage

```sh
eyeship compose                                 # looks for compose.yaml, docker-compose.yml… in the current directory
eyeship compose infra/compose.yaml              # a specific file
eyeship compose -f compose.yaml -f compose.prod.yaml   # several files, merged in order
eyeship compose --env-file .env.prod            # variables used for interpolation
eyeship compose --format json                   # machine-readable output
eyeship compose --format sarif                  # SARIF 2.1.0 (GitHub Code Scanning, IDEs)
eyeship compose --format html > report.html     # a self-contained page to share
eyeship compose --deployment prod               # a deployment declared in .eyeship.toml
eyeship compose --deployment prod --save        # also save the report in the history
eyeship ui                                      # local web interface on http://127.0.0.1:7878
eyeship compose --fail-on warning               # failure threshold: error (default), warning, info, none
eyeship compose --ignore R015,R016              # ignore rules (ignored findings are still counted)
eyeship rules                                   # list the rules
eyeship explain R001                            # failure scenario, fix, examples
```

### `.eyeship.toml` file

Placed in the directory where you run `eyeship`, it makes silencing explicit and versioned:

```toml
fail_on = "warning"          # failure threshold
ignore = ["R015", "R016"]    # for every service

[services.api]
ignore = ["R006"]            # for a single service
```

### Exit codes

| Code | Meaning |
|---|---|
| 0 | No finding at or above the threshold |
| 1 | At least one finding at or above the threshold |
| 2 | Usage or read error (missing file, invalid YAML) |

## Example

On [`examples/compose.yaml`](examples/compose.yaml):

```text
$ eyeship compose examples/compose.yaml
api
  error   R013 examples/compose.yaml:21:21
          variable 'DB_PASSWORD' is not set and has no default
          fix: define DB_PASSWORD in the environment or in .env, or use ${DB_PASSWORD:-default} or ${DB_PASSWORD:?message}
  warning R008 examples/compose.yaml:15:12
          image 'ghcr.io/acme/api:latest' is not pinned to a version and may change on the next pull
          fix: Pin 'acme/api' to an explicit version tag or a digest (@sha256:...).
  warning R001 examples/compose.yaml:17:9
          depends_on 'db' does not wait for the service to be healthy
          fix: Use `condition: service_healthy` for 'db' and give it a healthcheck.

worker
  error   R009 examples/compose.yaml:28:9
          host port 8080/tcp is already published by service 'api' (8080/tcp)
          fix: Publish a different host port, bind to a distinct host IP, or remove one of the two mappings of service 'api' and 'worker'.
  error   R010 examples/compose.yaml:29:16
          network 'backend' is not declared in the root 'networks' section
          fix: Declare 'backend' under the root 'networks' key (add 'external: true' if it is created outside this project), or fix the network name.

db
  error   R011 examples/compose.yaml:35:9
          named volume 'pgdata' is not declared in the root 'volumes' section
          fix: Declare 'pgdata' under the root 'volumes' key (add 'external: true' if it is created outside this project), or fix the volume name.

4 errors, 2 warnings, 0 info
$ echo $?
1
```

## Deployments and history

Declare each target environment once in `.eyeship.toml`; paths are relative to this file:

```toml
reports_dir = ".eyeship/reports"   # optional, where saved reports go (this is the default)

[deployments.staging]
files = ["compose.yaml", "compose.staging.yaml"]

[deployments.prod]
files = ["compose.yaml", "compose.prod.yaml"]
env_file = ".env.prod"             # optional
fail_on = "warning"                # optional, overrides the global threshold
```

`eyeship compose --deployment prod` then runs exactly the same audit as
`eyeship compose -f compose.yaml -f compose.prod.yaml --env-file .env.prod`.
Without any `[deployments]` section, there is a single `default` deployment: the Compose file found in the current directory.

`--save` (with an optional `--label`, such as a commit SHA) also writes the report as a SARIF file in
`<reports_dir>/<deployment>/<date>.sarif`. The standard output and the exit code do not change.
eyeship never deletes, archives or versions these files: keeping them is up to you.

## Web interface

```sh
eyeship ui                         # http://127.0.0.1:7878
eyeship ui --port 0                # any free port
eyeship ui --reports-dir ci-reports   # show a history downloaded from your CI
```

- **Deployments**: every declared deployment, audited when the page loads (edit a file, reload), failing ones first, with the trend since the last saved report.
- **Report**: findings grouped by service, filterable by severity and service; each rule links to its explanation.
- **History**: saved reports, a chart of findings per severity, and a comparison of two reports (new, resolved and unchanged findings).
- **Rules**: the same content as `eyeship rules` and `eyeship explain`.

The interface is read-only, works offline and runs no JavaScript. It listens on `127.0.0.1` by default and only answers
requests addressed to `localhost`. `--host 0.0.0.0` exposes your reports to anyone who can reach the machine: eyeship
prints a warning, and there is no authentication. In Docker, publish the port on the loopback only:
`docker run --rm -p 127.0.0.1:7878:7878 -v "$PWD:/work" ghcr.io/namularbre/eyeship ui --host 0.0.0.0`.

## GitHub Actions integration

```yaml
- run: docker run --rm -v "$PWD:/work" ghcr.io/namularbre/eyeship compose --fail-on warning
```

To show findings in GitHub Code Scanning, upload the SARIF output:

```yaml
- run: docker run --rm -v "$PWD:/work" ghcr.io/namularbre/eyeship compose --format sarif --fail-on none > eyeship.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: eyeship.sarif
```

## Rules

| ID | Severity | Rule |
|---|---|---|
| R001 | warning | depends_on without health condition |
| R002 | error | depends_on towards a service without healthcheck |
| R003 | error | Invalid dependency |
| R004 | warning | Inconsistent healthcheck |
| R005 | warning | No restart policy |
| R006 | warning | No memory limit |
| R007 | warning | Logs without rotation |
| R008 | warning | Image not pinned |
| R009 | error | Published port conflict |
| R010 | error | Undeclared network |
| R011 | error | Undeclared or malformed volume |
| R012 | warning | Missing host path |
| R013 | error | Undefined environment variable |
| R014 | error | Invalid container name |
| R015 | info | Fixed container name |
| R016 | info | Container name not in kebab-case |
| R017 | info | Obsolete version key |

`eyeship explain <ID>` details each one: the problem, the failure scenario, the fix, a faulty example and a corrected example. Rule IDs are stable: a rule may be deprecated, never renumbered.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --release -p eyeship-cli --test perf -- --ignored   # ENF1 (non-functional req.): < 100 ms for 50 services
scripts/bench.sh                                               # time and max RSS (requires GNU time)
docker build -t eyeship .
```

The approach (constitution, spec, plan, tasks) is described in [`docs/`](docs/) (in French). The tool's messages are in English.

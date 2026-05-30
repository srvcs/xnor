# srvcs-xnor

The logical-equivalence orchestrator of the srvcs.cloud distributed standard
library.

Its single concern: **exclusive NOR (equivalence) of two booleans** — true
exactly when `a == b`. It does no logic of its own. It asks
[`srvcs-xor`](https://github.com/srvcs/xor) for `a XOR b`, then asks
[`srvcs-not`](https://github.com/srvcs/not) to negate that verdict — yielding
`NOT(a XOR b)`.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Exclusive NOR (equivalence) of `a` and `b` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' -d '{"a": true, "b": true}'
# {"a":true,"b":true,"result":true}
```

Responses:

- `200 {"a": x, "b": y, "result": true | false}` — evaluated.
- `422` — invalid input, forwarded from a leaf dependency.
- `503` — a dependency is unavailable.

## Orchestration

```
x      = srvcs-xor  { "a": a, "b": b }   -> result (bool)
result = srvcs-not  { "value": x }       -> result (bool)
```

## Dependencies

- [`srvcs-xor`](https://github.com/srvcs/xor)
- [`srvcs-not`](https://github.com/srvcs/not)

This is an orchestrator over boolean leaf services; its operands are booleans.
Input validation propagates from the leaf dependencies via their `422`
responses; this service does not validate operands itself.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_XOR_URL` | `http://127.0.0.1:8080` | Base URL of `srvcs-xor` |
| `SRVCS_NOT_URL` | `http://127.0.0.1:8080` | Base URL of `srvcs-not` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up mock `srvcs-xor` and `srvcs-not` services
in-process, covering the truth table, a degraded dependency (`503`), and a
forwarded `422`. See [`srvcs/platform`](https://github.com/srvcs/platform) for
the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.

# Contributing to percli

Thanks for your interest in contributing. This document covers the development workflow.

## Getting Started

```bash
git clone https://github.com/kamiyoai/percli.git
cd percli
cargo build --workspace
cargo test --workspace
```

The on-chain crates (percli-chain, percli-program) require Solana SDK dependencies:

```bash
cargo build -p percli --features chain
cargo build -p percli-program
```

## Development Workflow

1. Fork the repository and create a feature branch from `master`.
2. Make your changes.
3. Run the checks:

```bash
# Format
cargo fmt --all

# Lint
cargo clippy --workspace --exclude percli-chain --exclude percli-program -- -D warnings

# Test
cargo test --workspace --exclude percli-chain --exclude percli-program

# Run bundled scenarios
for f in scenarios/*.toml; do cargo run -p percli -- sim "$f"; done
```

4. Open a pull request against `master`.

## Pull Request Checklist

- [ ] `cargo fmt --all` passes
- [ ] `cargo clippy` passes with no warnings
- [ ] All tests pass
- [ ] New functionality includes tests where applicable
- [ ] Bundled scenarios still run cleanly

## Adding a Scenario

Place new `.toml` scenarios in `scenarios/`. They will automatically be tested in CI. See existing scenarios for the format.

## Adding an Agent Example

Place agent configs in `examples/` with a descriptive name. Include a README comment at the top of the config file explaining what the agent does.

## Crate Overview

| Crate | Purpose |
|-------|---------|
| `percolator` (root) | Upstream risk engine — do not modify unless upstreaming |
| `percli-core` | Engine wrapper, scenario types, agent protocol |
| `percli` | CLI binary |
| `percli-chain` | Solana RPC commands |
| `percli-program` | Anchor on-chain program |
| `percli-wasm` | WebAssembly binding |

## Code Style

- Follow existing patterns in the codebase.
- Use `anyhow` for error handling in CLI code, `Result` with typed errors in library code.
- Keep commits focused — one logical change per commit.

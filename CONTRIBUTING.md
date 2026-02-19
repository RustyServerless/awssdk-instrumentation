# Contributing to awssdk-instrumentation

Whether you're here to report bugs, suggest features, improve documentation, or submit code, you're in the right place.

---

## Bug Reports

If you've found something that doesn't look right:

1. **Search existing issues** to see if it's already been reported.
2. If not, [open a new issue](https://github.com/RustyServerless/awssdk-instrumentation/issues/new) and provide:
   - A clear title and description.
   - Steps to reproduce the issue.
   - Expected vs actual behavior.
   - Minimal code example.

## Feature Requests

We welcome ideas that can improve the developer experience or extend functionality. When opening a feature request:

- Describe the problem you're trying to solve.
- Explain how your suggestion addresses it.
- Optionally, suggest an implementation path.

---

## Code Contributions

We welcome PRs for bug fixes, features, performance improvements, or refactors.

### 1. Fork and Clone

```sh
git clone https://github.com/YOUR_USERNAME/awssdk-instrumentation.git
cd awssdk-instrumentation
```

### 2. Set Up the Development Environment

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) with [direnv](https://direnv.net/) for a reproducible development environment. If you have both installed, entering the project directory will automatically:

- Install the correct Rust toolchain (as defined in `rust-toolchain.toml`)
- Install the git pre-commit hooks

If you don't use Nix/direnv, you can install the hooks manually:

```sh
./scripts/install-hooks.sh
```

Make sure all dependencies compile correctly:

```sh
cargo check
cargo test
```

**Important:** This project declares an MSRV (Minimum Supported Rust Version) of **1.85.0** in `Cargo.toml`. The development toolchain is intentionally newer. Clippy reads the `rust-version` field from `Cargo.toml` and will warn you if you use APIs or syntax that are not available in the MSRV. This is enforced both locally (via the pre-commit hook) and in CI.

### 3. Pre-commit Hooks

The pre-commit hook runs the following checks on every commit that touches Rust files:

1. **Formatting** -- `cargo fmt --check`
2. **Linting** -- `cargo clippy --all-targets --all-features -- -D warnings`
3. **Documentation** -- `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items`
4. **Tests** -- `cargo test --all-features`

If any check fails, the commit is aborted. Fix the issues and try again.

These are the same checks that run in CI, so **if your commit passes locally, it will pass CI**.

### 4. Make Your Changes

- Write clean, idiomatic Rust.
- Add or update tests if relevant.
- Document new public APIs using `///` doc comments.

### 5. Submit the Pull Request

- Describe what you've changed and why.
- Link the issue you're fixing if applicable.
- Be open to feedback and iteration.

---

## CI Pipeline

GitHub Actions runs three parallel jobs on every push and pull request:

| Job | Toolchain | What it checks |
|---|---|---|
| **Lint** | stable | `cargo fmt`, `cargo clippy` (with MSRV enforcement), `cargo doc` |
| **Test** | stable | `cargo test` |
| **MSRV Check** | 1.85.0 | `cargo check` and `cargo test` compile and pass on the declared MSRV |

Clippy runs on **stable** (not on the MSRV toolchain) so that the `incompatible_msrv` lint can detect usage of APIs introduced after the declared MSRV.

---

## Code Quality Checklist

Before submitting a PR:

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` reports no warnings
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items` succeeds
- [ ] `cargo test --all-features` passes
- [ ] New public APIs are documented
- [ ] Feature or bugfix is covered by tests (if applicable)

---

## Releasing

Only maintainers can publish new versions. When a release is due:

- Bump the version in `Cargo.toml`.
- Update the `CHANGELOG.md`.
- Tag the release commit with `vX.Y.Z`.

---

## Community and Support

Have questions? Want to discuss implementation strategies or architecture?

- Open a [GitHub Discussion](https://github.com/RustyServerless/awssdk-instrumentation/discussions)
- Comment on open issues/PRs

---

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

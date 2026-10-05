# Contributing

## Prerequisites

- Node 24
- npm 12
- [rustup](https://rustup.rs/), which installs the pinned Rust toolchain

## Setup

Run `npm ci`. Its `prepare` script installs the git hooks, the pinned Trivy and actionlint binaries, the Rust lint tools the checks need, and the minimum supported Rust toolchain.

## Pull requests

These checks and tests must pass before a pull request is opened:

```sh
npm run check
npm run unit
npm run integration
```

## Tests

Rust unit tests sit beside their implementation in `*.test.rs` files, included by a private `#[cfg(test)]` module with `#[path = "filename.test.rs"]`. Tests that drive a mock Jev server over HTTP go in `*.integration.test.rs` files, included under a module named `integration`.

## Commits

Commits follow [Conventional Commits](https://www.conventionalcommits.org/), and the commit-msg hook enforces it. A commit body states the reasoning for the change and links the code, changes and commits it references with [Inspectory](https://github.com/visionsofparadise/inspectory) links.

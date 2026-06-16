# code2graph — Python bindings

Python bindings to the [code2graph](https://github.com/nodedb-lab/code2graph) Rust library, which turns
source files into structural facts: symbols (definitions), references (use sites), and cross-file edges.

## Build

Inside a virtualenv, from the `bindings/python` directory:

```sh
pip install maturin
maturin develop
```

This compiles the Rust extension and installs it into the active virtualenv.

## Usage

```python
import code2graph
facts = code2graph.extract("src/lib.rs", "pub fn hello() {}")
print(facts["symbols"])
```

`extract(file, source)` returns a dict mirroring the `FileFacts` schema (keys: `symbols`, `references`,
`scopes`, `bindings`, `ffi_exports`). `SymbolId` values appear as their stable SCIP strings.

## Forthcoming

`build_graph` (multi-file resolution to a `CodeGraph` with typed edges and `Confidence` scores) is coming
in a follow-up unit. The resolver tiers — `SymbolTableResolver` (fast, name-only) and
`ScopeGraphResolver` (scope-aware, higher precision) — will be exposed once the single-file extraction
binding is stable.

# code2graph — Python bindings

Python bindings to the [code2graph](https://github.com/nodedb-lab/code2graph) Rust library, which turns
source files into structural facts: symbols (definitions), references (use sites), and cross-file edges.

## Build

Inside a virtualenv, from the `bindings/python` directory:

```sh
pip install maturin
maturin develop
```

This compiles the Rust extension and installs it into the active virtualenv. The published distribution is named `code2graph-rs` (`pip install code2graph-rs`); the import name is `code2graph`.

## Usage

```python
import code2graph
facts = code2graph.extract("src/lib.rs", "pub fn hello() {}")
print(facts["symbols"])
```

`extract(file, source)` returns a dict mirroring the `FileFacts` schema (keys: `symbols`, `references`,
`scopes`, `bindings`, `ffi_exports`). `SymbolId` values appear as their stable SCIP strings.

Resolve facts from multiple files into a cross-file graph with `build_graph`:

```python
import code2graph
a = code2graph.extract("src/util.rs", "pub fn helper() {}")
b = code2graph.extract("src/main.rs", "pub fn run() { helper() }")
graph = code2graph.build_graph([a, b], tier="name")
print(graph["edges"])  # each edge: from, to, role, confidence, provenance, occ
```

`build_graph(files, tier="name")` returns a dict mirroring `CodeGraph` (`symbols` + `edges`). The `tier`
argument selects the resolver: `"name"` (default, Tier A — fast, recall-first, `NameOnly` confidence) or
`"scope"` (Tier B — scope-graph path resolution, `Scoped`/`Exact` confidence). The helper
`language_of(path)` returns the canonical language tag for a path (e.g. `"rust"`), or `None` if the
extension is unrecognized.

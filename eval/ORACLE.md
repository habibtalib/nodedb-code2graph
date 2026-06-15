# SCIP oracle fixtures

The eval harness scores code2graph's ref→def resolution against an **external
SCIP oracle** — an index produced by a mature, type-aware indexer (e.g.
`scip-typescript`). This is how we turn "best at code→graph" into a number:
precision = "are our edges correct?", recall = "how much of the type-aware truth
do we recover syntactically?".

## How it stays reproducible and dependency-free

Two paths, cleanly split:

- **Scoring (always on, no external deps).** Each oracle case commits a binary
  `index.scip` (provenance) and a derived text `oracle.edges` (the location-only
  ref→def pairs). The default harness reads `oracle.edges` and scores against it.
  `cargo test` / `cargo run -p code2graph-eval` pull **no** SCIP/protobuf deps and
  run **no** external tools.
- **Generation (gated, maintainer-only).** Re-deriving `oracle.edges` from
  `index.scip` is behind the off-by-default `oracle-regen` feature, which is the
  only thing that compiles `scip`/`protobuf`. The default build never sees them.

So the flaky, polyglot indexers only ever touch a committed artifact; nothing in
the normal build or test loop depends on them.

## Case layout

```
eval/corpus/<lang>_oracle/<case>/
    <sources…>      # the exact files the indexer ran on (code2graph extracts these too)
    index.scip      # committed binary index (provenance / regeneration input)
    oracle.edges    # committed, derived: `<ref>:<line> <def>:<line>` location pairs
```

A case with `oracle.edges` is scored location-only (SCIP occurrence roles don't
map to our Call/TypeRef taxonomy), via `score_oracle`. Cases with the usual
`expected.edges` keep role-typed scoring.

## Regenerating an oracle (TypeScript example)

Done in a throwaway directory so `node_modules` never lands near the corpus:

```sh
# 1. Stage the sources + a minimal TS project in a temp dir
mkdir -p /tmp/scip-gen/src && cp eval/corpus/ts_oracle/scoped_call/*.ts /tmp/scip-gen/src/
cd /tmp/scip-gen
cat > package.json <<'JSON'
{ "name": "scip-gen", "private": true,
  "devDependencies": { "typescript": "5.4.5", "@sourcegraph/scip-typescript": "0.3.14" } }
JSON
cat > tsconfig.json <<'JSON'
{ "compilerOptions": { "target": "ES2020", "module": "CommonJS", "strict": true,
    "rootDir": "src", "moduleResolution": "node" }, "include": ["src"] }
JSON
npm install --no-audit --no-fund
npx scip-typescript index --output index.scip

# 2. Copy the index back into the committed case
cp index.scip "$OLDPWD/eval/corpus/ts_oracle/scoped_call/index.scip"
cd "$OLDPWD"

# 3. Derive oracle.edges (the only step that compiles scip/protobuf)
cargo run -p code2graph-eval --features oracle-regen --bin gen-oracle -- \
    eval/corpus/ts_oracle/scoped_call
```

Commit the updated `index.scip` + `oracle.edges`; never commit `node_modules`.

## Adding another language

Install that language's SCIP indexer once (e.g. `scip-python`,
`rust-analyzer scip`), index a small fixture, commit `index.scip`, run
`gen-oracle` on the case dir. The scoring path needs no changes — any
`*_oracle/<case>/` dir with an `oracle.edges` is picked up automatically.

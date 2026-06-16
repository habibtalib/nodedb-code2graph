# code2graph — Node.js / Bun bindings

Node.js and Bun native-addon bindings to the [code2graph](https://github.com/nodedb-lab/code2graph) Rust library, built with [napi-rs](https://napi.rs). Given source files in any supported language, it produces symbols, references, and cross-file edges as plain JS objects — the same neutral facts as the Rust crate, with no storage opinion.

## Build

```sh
npm install
npm run build:debug   # debug build — emits platform .node + index.js + index.d.ts
npm run build         # release build
```

The `napi build` command (from `@napi-rs/cli`) compiles the Rust crate and writes three files into `bindings/node/`: the platform-native `.node` addon, `index.js` (the JS loader), and `index.d.ts` (TypeScript declarations).

## Usage

napi-rs automatically converts Rust `snake_case` function names to JS `camelCase` (`build_graph` → `buildGraph`, `language_of` → `languageOf`). TypeScript types are generated as `index.d.ts`.

```js
const { extract, buildGraph, languageOf } = require("code2graph");

const facts = extract("src/lib.rs", "pub fn hello() {}");
const graph = buildGraph([facts], "name");
console.log(graph.edges);

console.log(languageOf("src/main.go")); // "go"
console.log(languageOf("unknown.xyz")); // null
```

The `tier` argument to `buildGraph` is `"name"` (default, Tier A — fast, recall-first, `NameOnly` confidence) or `"scope"` (Tier B — scope-graph path resolution, `Scoped`/`Exact` confidence).

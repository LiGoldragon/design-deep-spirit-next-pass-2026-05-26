# ARCHITECTURE — design-deep-spirit-next-pass

## Build-time shape

```text
schema/signal-frame.schema  ← macro signature declarations
schema/spirit.schema        ← user surfaces + (Route Input) etc. macro calls
                                      │
                                      ▼
       build.rs (uses schema-next designer-finish-macro-engine branch)
                                      │
                                      ▼ lower_source
            schema-next SchemaEngine + MacroRegistry
                                      │
                                      ▼ fixed-point expand
                Asschema { namespace: [
                    user types ...,
                    InputRoute (Enum),       ← from Route macro expansion
                    OutputRoute (Enum),      ← from Route macro expansion
                    ApexCodec (Struct),      ← from SignalCodec macro expansion
                    FrameError (Enum),       ← imported from signal-frame.schema
                ], surfaces: [Input, Output], macros: [Route, SignalCodec] }
                                      │
                                      ▼ build.rs RustEmitter
                            OUT_DIR/spirit_generated.rs
                                      │
                                      ▼ include!() in src/lib.rs
                                    crate::generated
```

## Runtime shape — the signal/executor/SEMA triad

Per designer/371 the runtime is a three-plane triad. Each plane's
responsibility lives in a single Rust module:

| Plane | Module | Responsibility |
|---|---|---|
| Signal | `src/signal.rs` | Length-prefix wire envelope + emitted signal-frame codec calls |
| Executor | `src/executor.rs` | `Engine` actor + methods on emitted nouns |
| SEMA | `src/sema.rs` | Single-writer redb actor + topic indexing + schema-version migration |

## Module structure

| Path | Role |
|---|---|
| `schema/signal-frame.schema` | Macro signature declarations + FrameError |
| `schema/spirit.schema` | User surfaces + macro calls |
| `build.rs` | Schema engine driver + ExpansionMacro impls + local RustEmitter |
| `src/lib.rs` | Public re-exports + `include!()` of generated source |
| `src/signal.rs` | Length-prefix shim over emitted signal-frame codec |
| `src/executor.rs` | Engine + ObservationProjection + TopicCounter (all methods on impl blocks) |
| `src/sema.rs` | SemaActor + redb tables + StoredRecord + DaemonClock |
| `src/daemon.rs` | Unix-socket server gluing signal → executor |
| `src/bin/spirit.rs` | Thin CLI: NOTA argv → daemon → NOTA stdout |
| `src/bin/spirit-daemon.rs` | Daemon binary |
| `tests/macro_expansion_visible.rs` | Pins the macro-expanded namespace contents |
| `tests/end_to_end.rs` | Real socket; record + observe + state |

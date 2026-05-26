# INTENT — design-deep-spirit-next-pass

Designer-parallel deep spirit substrate proving the finished
schema-next macro engine (designer-assistant/375). Distinct from
sibling repo `design-deep-spirit-2026-05-26` (designer-assistant/374),
which hand-rolled around the macro engine gaps; this repo runs ON TOP
of the completed engine.

## Provenance

Per psyche 2026-05-26 ("do another pass, make it more real — finish
the schema macro design and use it"). Captured as intent records
865, 866, 867 through the deployed Spirit CLI.

The companion feature branch is
[`LiGoldragon/schema-next` → `designer-finish-macro-engine-2026-05-26`](https://github.com/LiGoldragon/schema-next/tree/designer-finish-macro-engine-2026-05-26)
— this repo's `Cargo.toml` points at that branch directly.

## What this repo PROVES

1. The schema macro engine actually expands `(Route Input)` /
   `(Route Output)` / `(SignalCodec Input Output)` namespace entries
   into typed declarations at lower time.
2. `(ImportAll [signal-frame.schema])` actually resolves to filesystem
   loading + namespace merging, with conflict detection.
3. The lowered asschema carries the macro-expanded types in
   `namespace()` ALONGSIDE the user-authored types.
4. A v0.3-capability Spirit runtime (Record + Observe + State, multi-
   topic Entry shape, observation modes, daemon-stamped timestamps,
   redb durable storage) compiles + runs entirely off the macro-
   expanded asschema.

The Route enums + signal-frame methods are NOT hand-rolled inside
`schema-rust-next`; the design repo's local emitter consumes the
asschema directly + emits them based on the macro-expanded enums in
the namespace.

## Deletion target

This repo deletes when the schema-next macro engine + a corresponding
emitter pattern land in operator's main of `schema-rust-next`.
Specifically, deletion happens when:

1. `schema-next` main absorbs the macro engine completion (the
   feature branch merges).
2. `schema-rust-next` main learns to consume macro-expanded asschemas
   the way this repo's local emitter does.
3. Operator-canonical `spirit-next` switches to authoring `(Route
   Input)` / `(Route Output)` macro CALLS in `schema/spirit.schema`
   rather than depending on schema-rust-next's hardcoded signal-frame
   emission.

The git history preserves the design substrate for archaeology.

## Coordination with sibling design-deep-spirit-2026-05-26

The sibling repo (`design-deep-spirit-2026-05-26`,
designer-assistant/374) landed a working v0.3-capability runtime
WITHOUT the schema macro engine being real. It depended on operator
`schema-rust-next`'s hardcoded signal-frame emission.

This repo (`design-deep-spirit-next-pass-2026-05-26`) lands the same
v0.3-capability runtime WITH the macro engine. The intent is
complementary, not competing — when the macro engine integrates into
schema-rust-next, the sibling's pattern can adopt the macro-driven
emission seamlessly because the surface shape is identical.

## Per skills

- `skills/double-implementation-strategy.md` §"design-prefix discipline"
- `skills/major-break-via-new-repo.md` (the new-repo scaffold pattern)
- `skills/component-triad.md` §"The single argument rule" (the CLI
  takes only a NOTA argument, no flags)
- `AGENTS.md` hard overrides (methods-on-impl-blocks; NOTA bracket-
  only; jj headless)

# design-deep-spirit-next-pass-2026-05-26

Designer-parallel deep spirit substrate proving the schema-next macro
engine completion (designer-assistant/375).

See `INTENT.md` for provenance, deletion target, and the schema/runtime
shape. See `ARCHITECTURE.md` for the module layout.

This repo runs the v0.3-capability Spirit runtime on top of the
finished macro engine — the `Route` + `SignalCodec` enum + struct
declarations come from FIXED-POINT EXPANSION of imported signal-frame
macros, not from hardcoded emission in schema-rust-next.

//! Tests pinning the build-time macro engine behavior. These verify
//! that the schema engine's fixed-point expansion + import resolution
//! produced the expected `Asschema` shape, by inspecting the
//! generated source string + the canonical NOTA artifact.

const GENERATED_SOURCE: &str = include_str!(concat!(env!("OUT_DIR"), "/spirit_generated.rs"));
const CANONICAL_NOTA: &str = include_str!(concat!(env!("OUT_DIR"), "/spirit.asschema.nota"));
const SCHEMA_HASH: &str = include_str!(concat!(env!("OUT_DIR"), "/spirit.schema.hash"));

#[test]
fn generated_source_carries_macro_expanded_input_route_enum() {
    assert!(
        GENERATED_SOURCE.contains("pub enum InputRoute"),
        "InputRoute enum from (Route Input) macro expansion must be emitted"
    );
    assert!(
        GENERATED_SOURCE.contains("pub enum OutputRoute"),
        "OutputRoute enum from (Route Output) macro expansion must be emitted"
    );
    assert!(
        GENERATED_SOURCE.contains("pub struct ApexCodec"),
        "ApexCodec struct from (SignalCodec Input Output) macro expansion must be emitted"
    );
}

#[test]
fn generated_source_carries_imported_frame_error_from_signal_frame_schema() {
    // FrameError is declared in signal-frame.schema; spirit.schema
    // imports it via `(ImportAll [signal-frame.schema])`. The import
    // resolution lands FrameError in the consumer's namespace.
    assert!(
        GENERATED_SOURCE.contains("pub enum FrameError"),
        "FrameError must be imported from signal-frame.schema"
    );
}

#[test]
fn generated_source_carries_signal_frame_methods_on_surfaces() {
    // The emitter detected the macro-expanded InputRoute + OutputRoute
    // enums and bridged them via impl blocks.
    assert!(
        GENERATED_SOURCE.contains("fn encode_signal_frame"),
        "encode_signal_frame method must be emitted"
    );
    assert!(
        GENERATED_SOURCE.contains("fn decode_signal_frame"),
        "decode_signal_frame method must be emitted"
    );
    assert!(
        GENERATED_SOURCE.contains("fn short_header"),
        "short_header method must be emitted"
    );
    assert!(
        GENERATED_SOURCE.contains("pub mod short_header"),
        "short_header constants module must be emitted"
    );
}

#[test]
fn canonical_nota_documents_macro_signatures_round_trippably() {
    assert!(
        CANONICAL_NOTA.contains("(Macro Route SurfaceEnum -> RouteEnum)"),
        "Route macro signature must round-trip canonically"
    );
    assert!(
        CANONICAL_NOTA.contains("(Macro SignalCodec InputSurface OutputSurface -> Codec)"),
        "SignalCodec macro signature must round-trip canonically"
    );
}

#[test]
fn canonical_nota_documents_macro_expanded_declarations() {
    assert!(
        CANONICAL_NOTA.contains("(Enum InputRoute (Variant Record) (Variant Observe) (Variant State))"),
        "InputRoute enum (from macro expansion) must appear in canonical NOTA"
    );
    assert!(
        CANONICAL_NOTA.contains("(Struct ApexCodec (Field [input] [Input]) (Field [output] [Output]))"),
        "ApexCodec struct (from macro expansion) must appear in canonical NOTA"
    );
}

#[test]
fn canonical_nota_documents_imported_declarations() {
    assert!(
        CANONICAL_NOTA.contains("(Enum FrameError"),
        "FrameError (imported from signal-frame.schema) must appear in canonical NOTA"
    );
    assert!(
        CANONICAL_NOTA.contains("(Import [SignalFrame]"),
        "the SignalFrame import must appear in the asschema's Imports section"
    );
}

#[test]
fn schema_hash_matches_environment_variable() {
    // The hash written to OUT_DIR matches the env var emitted by
    // build.rs's `cargo:rustc-env=` line.
    let env_hash: &str = env!("DESIGN_DEEP_SPIRIT_NEXT_PASS_SCHEMA_HASH");
    assert_eq!(
        SCHEMA_HASH.trim(),
        env_hash,
        "OUT_DIR hash must match build-time env var"
    );
}

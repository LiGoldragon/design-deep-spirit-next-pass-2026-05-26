//! Tests proving the schema macro engine RAN and the expansion
//! results are visible in the generated source.

use design_deep_spirit_next_pass::{ApexCodec, Input, InputRoute, Output, OutputRoute};

#[test]
fn input_route_enum_came_from_macro_expansion() {
    let _record = InputRoute::Record;
    let _observe = InputRoute::Observe;
    let _state = InputRoute::State;
}

#[test]
fn output_route_enum_came_from_macro_expansion() {
    let _accepted = OutputRoute::RecordAccepted;
    let _observed = OutputRoute::RecordsObserved;
    let _topics = OutputRoute::TopicsObserved;
    let _state = OutputRoute::StateObserved;
    let _error = OutputRoute::Error;
}

#[test]
fn apex_codec_struct_came_from_macro_expansion() {
    let _codec = ApexCodec {
        input: Input::State(design_deep_spirit_next_pass::generated::StateRequest::Topics),
        output: Output::Error(design_deep_spirit_next_pass::generated::ErrorMessage(
            "test".to_owned(),
        )),
    };
}

#[test]
fn surface_input_carries_macro_emitted_signal_frame_methods() {
    let input = Input::State(design_deep_spirit_next_pass::generated::StateRequest::Topics);
    let route = input.route();
    assert!(matches!(route, InputRoute::State));
    let header = input.short_header();
    assert_ne!(header, 0, "short header must be non-zero per surface layout");
}

#[test]
fn surface_input_signal_frame_round_trips() {
    let input = Input::State(design_deep_spirit_next_pass::generated::StateRequest::Topics);
    let frame = input.encode_signal_frame().expect("encode");
    let (route, decoded) = Input::decode_signal_frame(&frame).expect("decode");
    assert!(matches!(route, InputRoute::State));
    assert_eq!(input, decoded);
}

#[test]
fn surface_output_signal_frame_round_trips() {
    let output = Output::Error(design_deep_spirit_next_pass::generated::ErrorMessage(
        "test error".to_owned(),
    ));
    let frame = output.encode_signal_frame().expect("encode");
    let (route, decoded) = Output::decode_signal_frame(&frame).expect("decode");
    assert!(matches!(route, OutputRoute::Error));
    assert_eq!(output, decoded);
}

#[test]
fn schema_hash_is_emitted_into_environment() {
    let hash: &str = env!("DESIGN_DEEP_SPIRIT_NEXT_PASS_SCHEMA_HASH");
    assert_eq!(hash.len(), 64, "blake3 hex is 64 chars");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash must be hex"
    );
}

#[test]
fn nota_codec_round_trips_input() {
    use std::str::FromStr;
    let input = Input::State(design_deep_spirit_next_pass::generated::StateRequest::Topics);
    let nota = input.to_nota();
    let recovered = Input::from_str(&nota).expect("parse");
    assert_eq!(input, recovered);
}

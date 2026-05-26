//! End-to-end test: real Unix-socket daemon + real Input through the
//! signal-frame envelope + real SemaActor with redb on disk + real
//! Output reply. Proves the v0.3 capability shape works through
//! schema-emitted everything.

use std::str::FromStr;

use design_deep_spirit_next_pass::{
    ExchangeClient, Input, KindQuery, ObservationMode, ObserveSelection, Output, OutputRoute,
    RecordEntry, RecordList, StateRequest, TopicQuery,
    executor::EntryBuilder,
    generated::{Kind, Magnitude},
    run_daemon,
};

#[test]
fn record_observe_state_round_trip_through_socket() {
    let temp = tempfile::tempdir().expect("tempdir");
    let socket = temp.path().join("spirit.sock");
    let database = temp.path().join("spirit.redb");

    let daemon = run_daemon(&socket, &database).expect("start daemon");

    // Record three entries: two with topic "schema", one with topic "spirit".
    let entry_a = EntryBuilder::new_single_topic(
        "schema",
        Kind::Constraint,
        "schema language must support vector references",
        Magnitude::High,
    );
    let entry_b = EntryBuilder::new_single_topic(
        "spirit",
        Kind::Decision,
        "designer parallel deep next-pass implementation",
        Magnitude::Maximum,
    );
    let entry_c = EntryBuilder::new_single_topic(
        "schema",
        Kind::Principle,
        "macro engine must run fixed-point expansion",
        Magnitude::Maximum,
    );

    for entry in [entry_a, entry_b, entry_c] {
        let (route, output) =
            ExchangeClient::exchange(&socket, &Input::Record(entry)).expect("record");
        assert!(matches!(route, OutputRoute::RecordAccepted));
        assert!(matches!(output, Output::RecordAccepted(_)));
    }

    // Observe by topic schema, DescriptionOnly.
    let selection = ObserveSelection {
        topic_query: TopicQuery::TopicMatch(design_deep_spirit_next_pass::generated::Topic(
            "schema".to_owned(),
        )),
        kind_query: KindQuery::NoKind,
        observation_mode: ObservationMode::DescriptionOnly,
    };
    let (route, output) =
        ExchangeClient::exchange(&socket, &Input::Observe(selection)).expect("observe");
    assert!(matches!(route, OutputRoute::RecordsObserved));
    let RecordList(entry) = match output {
        Output::RecordsObserved(list) => list,
        unexpected => panic!("expected RecordsObserved, got {unexpected:?}"),
    };
    matches!(entry, RecordEntry::DescriptionOnly(_));

    // State / Topics.
    let (route, output) = ExchangeClient::exchange(
        &socket,
        &Input::State(StateRequest::Topics),
    )
    .expect("state");
    assert!(matches!(route, OutputRoute::StateObserved));
    assert!(matches!(output, Output::StateObserved(_)));

    daemon.shutdown().expect("shutdown");
}

#[test]
fn nota_input_parses_through_emitted_codec() {
    let nota = "(State Topics)";
    let input = Input::from_str(nota).expect("parse");
    assert!(matches!(
        input,
        Input::State(StateRequest::Topics)
    ));
    let round_trip = input.to_nota();
    let recovered = Input::from_str(&round_trip).expect("re-parse");
    assert_eq!(input, recovered);
}

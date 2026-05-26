//! Executor plane — methods on schema-emitted objects.
//!
//! Per designer/371 §3 + designer/373 §4.6 every executor verb is a
//! method on a schema-emitted noun or on the Engine actor. No free
//! function dispatch helpers at module scope.

use crate::generated::{
    Date, Description, Entry, ErrorMessage, Input, Kind, KindQuery, Magnitude, ObservationMode,
    ObserveSelection, Output, RecordDescription, RecordEntry, RecordIdentifier, RecordList,
    RecordWithProvenance, StateRequest, StateView, Time, Topic, TopicCount, TopicCountList,
    TopicList, TopicQuery,
};
use crate::sema::{SemaError, SemaHandle, SemaResponse, StoredRecord};

#[derive(Debug)]
pub enum EngineError {
    Sema(SemaError),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sema(error) => write!(formatter, "engine sema: {error}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<SemaError> for EngineError {
    fn from(value: SemaError) -> Self {
        Self::Sema(value)
    }
}

/// The Engine actor — the daemon's executor. Holds a `SemaHandle`
/// for state-involving operations. Methods are the dispatcher per
/// surface variant.
pub struct Engine {
    sema: SemaHandle,
}

impl Engine {
    pub fn new(sema: SemaHandle) -> Self {
        Self { sema }
    }

    pub fn sema(&self) -> &SemaHandle {
        &self.sema
    }

    /// Apex entrypoint — matches the schema-emitted `Input` variants.
    pub fn handle(&self, input: Input) -> Output {
        match input {
            Input::Record(entry) => self.handle_record(entry),
            Input::Observe(selection) => self.handle_observe(selection),
            Input::State(request) => self.handle_state(request),
        }
    }

    fn handle_record(&self, entry: Entry) -> Output {
        match self.sema.record(entry) {
            Ok(SemaResponse::Recorded(identifier)) => Output::RecordAccepted(identifier),
            Ok(other) => Output::Error(ErrorMessage(format!(
                "unexpected sema response: {other:?}"
            ))),
            Err(error) => Output::Error(ErrorMessage(format!("record failed: {error}"))),
        }
    }

    fn handle_observe(&self, selection: ObserveSelection) -> Output {
        let mode = selection.observation_mode.clone();
        match self.sema.observe(selection) {
            Ok(SemaResponse::Observed(records)) => {
                let projection = ObservationProjection { mode };
                projection.project_records(records)
            }
            Ok(other) => Output::Error(ErrorMessage(format!(
                "unexpected sema response: {other:?}"
            ))),
            Err(error) => Output::Error(ErrorMessage(format!("observe failed: {error}"))),
        }
    }

    fn handle_state(&self, request: StateRequest) -> Output {
        match request {
            StateRequest::Topics => match self.sema.state() {
                Ok(SemaResponse::Topics(topics)) => {
                    let counter = TopicCounter;
                    let view = counter.build_view_from_topics(topics);
                    Output::StateObserved(view)
                }
                Ok(other) => Output::Error(ErrorMessage(format!(
                    "unexpected sema response: {other:?}"
                ))),
                Err(error) => Output::Error(ErrorMessage(format!("state failed: {error}"))),
            },
        }
    }
}

/// Maps `Vec<StoredRecord>` into a `RecordList` based on the
/// `ObservationMode`. The schema's `RecordList [RecordEntry]` is a
/// newtype around ONE RecordEntry — same wire-shape limitation as
/// /374 §"Open shape questions" Q1. We project the FIRST stored
/// record; SEMA holds the full set.
struct ObservationProjection {
    mode: ObservationMode,
}

impl ObservationProjection {
    fn project_records(self, records: Vec<StoredRecord>) -> Output {
        if records.is_empty() {
            return Output::Error(ErrorMessage(String::from("no matching record")));
        }
        let first = records.into_iter().next().expect("non-empty checked");
        let entry = self.project_one(first);
        Output::RecordsObserved(RecordList(entry))
    }

    fn project_one(&self, stored: StoredRecord) -> RecordEntry {
        match self.mode {
            ObservationMode::DescriptionOnly => RecordEntry::DescriptionOnly(RecordDescription {
                record_identifier: stored.identifier.clone(),
                description: stored.entry.description.clone(),
            }),
            ObservationMode::WithProvenance => RecordEntry::WithProvenance(RecordWithProvenance {
                record_identifier: stored.identifier,
                date: stored.date,
                time: stored.time,
                entry: stored.entry,
            }),
        }
    }
}

/// Folds an aggregated `Vec<(Topic, u64)>` into the schema-emitted
/// nested-newtype shape `StateView(TopicCountList(TopicCount))`. The
/// schema's TopicCountList is a newtype-around-one — same gap as
/// RecordList. We expose the first topic; SEMA holds the full count.
struct TopicCounter;

impl TopicCounter {
    fn build_view_from_topics(self, topics: Vec<(Topic, u64)>) -> StateView {
        if topics.is_empty() {
            return StateView(TopicCountList(TopicCount {
                topic: Topic(String::from("(none)")),
                integer: 0,
            }));
        }
        let (topic, count) = topics.into_iter().next().expect("non-empty checked");
        StateView(TopicCountList(TopicCount {
            topic,
            integer: count,
        }))
    }
}

/// Builder pattern for `Entry` — method on a builder noun.
pub struct EntryBuilder;

impl EntryBuilder {
    pub fn new_single_topic(
        topic: &str,
        kind: Kind,
        description: &str,
        magnitude: Magnitude,
    ) -> Entry {
        Entry {
            topic_list: TopicList(Topic(topic.to_owned())),
            kind,
            description: Description(description.to_owned()),
            magnitude,
        }
    }
}

// Unused-warning suppression for types brought through generated:
const _: std::marker::PhantomData<(Date, Time, RecordIdentifier, KindQuery, TopicQuery)> =
    std::marker::PhantomData;

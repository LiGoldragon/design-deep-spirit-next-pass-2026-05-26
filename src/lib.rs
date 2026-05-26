//! `design-deep-spirit-next-pass` — designer-assistant/375 substrate
//! consuming the finished schema-next macro engine.
//!
//! The schema lowering happens at build time in `build.rs`:
//!   1. `schema-next` (designer-finish-macro-engine-2026-05-26 branch)
//!      lowers `schema/spirit.schema`.
//!   2. The schema's `{ SignalFrame (ImportAll [signal-frame.schema]) }`
//!      causes the engine to resolve the import + load the macro
//!      declarations from `signal-frame.schema`.
//!   3. The namespace's `(Route Input)` / `(Route Output)` /
//!      `(SignalCodec Input Output)` entries are macro CALLS — the
//!      engine resolves them through the registered ExpansionMacro
//!      impls and produces `InputRoute`, `OutputRoute`, `ApexCodec`
//!      types in the lowered asschema's namespace.
//!   4. The build.rs's local RustEmitter consumes the macro-expanded
//!      asschema to produce Rust source.
//!
//! At RUNTIME, the daemon + CLI run on top of the emitted source.
//!
//! The signal/executor/SEMA runtime triad is visible in the module
//! structure (per designer/371):
//!
//! - [`signal`] — wire framing perimeter (length-prefix shim).
//! - [`executor`] — methods on emitted objects + Engine actor.
//! - [`sema`] — single-writer redb actor with topic indexes.

#![forbid(unsafe_code)]

pub mod daemon;
pub mod executor;
pub mod sema;
pub mod signal;

pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/spirit_generated.rs"));
}

pub use daemon::{DaemonError, run_daemon};
pub use executor::{Engine, EngineError};
pub use generated::{
    ApexCodec, Date, Description, Entry, ErrorMessage, Input, InputRoute, Kind, KindQuery,
    Magnitude, ObservationMode, ObserveSelection, Output, OutputRoute, RecordDescription,
    RecordEntry, RecordIdentifier, RecordList, RecordWithProvenance, SignalFrameError,
    StateRequest, StateView, Time, Topic, TopicCount, TopicCountList, TopicList, TopicQuery,
};
pub use sema::{SemaActor, SemaCommand, SemaError, SemaHandle, SemaResponse, StoredRecord};
pub use signal::{ExchangeClient, LengthPrefix, TransportError};

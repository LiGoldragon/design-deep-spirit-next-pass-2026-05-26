//! SEMA plane — single-writer redb actor.
//!
//! Per designer/371 §4 + operator/209 the SEMA plane owns durable
//! storage + identifier mint + topic indexing + observation queries.
//! All writes flow through this one actor; the runtime asserts only
//! one actor is spawned per database.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::{self, JoinHandle},
};

use redb::{Database, ReadableTable, TableDefinition};
use rkyv::rancor::Error as RkyvError;

use crate::generated::{
    Date, Description, Entry, Kind, KindQuery, Magnitude, ObservationMode, ObserveSelection, Time,
    Topic, TopicCount, TopicCountList, TopicList, TopicQuery, RecordIdentifier, StateView,
};

const RECORDS_TABLE: TableDefinition<u64, Vec<u8>> = TableDefinition::new("records");
const META_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("meta");

pub static SEMA_ACTOR_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum SemaError {
    Redb(String),
    Rkyv(String),
    Channel(String),
    SchemaVersionMismatch { found: String, expected: String },
}

impl std::fmt::Display for SemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Redb(message) => write!(formatter, "redb error: {message}"),
            Self::Rkyv(message) => write!(formatter, "rkyv error: {message}"),
            Self::Channel(message) => write!(formatter, "channel error: {message}"),
            Self::SchemaVersionMismatch { found, expected } => write!(
                formatter,
                "schema version mismatch: found {found}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for SemaError {}

impl From<redb::Error> for SemaError {
    fn from(value: redb::Error) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<redb::TransactionError> for SemaError {
    fn from(value: redb::TransactionError) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<redb::TableError> for SemaError {
    fn from(value: redb::TableError) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<redb::StorageError> for SemaError {
    fn from(value: redb::StorageError) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<redb::CommitError> for SemaError {
    fn from(value: redb::CommitError) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<redb::DatabaseError> for SemaError {
    fn from(value: redb::DatabaseError) -> Self {
        Self::Redb(value.to_string())
    }
}

impl From<RkyvError> for SemaError {
    fn from(value: RkyvError) -> Self {
        Self::Rkyv(value.to_string())
    }
}

impl From<std::io::Error> for SemaError {
    fn from(value: std::io::Error) -> Self {
        Self::Redb(value.to_string())
    }
}

/// A record as stored in SEMA — the wire `Entry` plus daemon-stamped
/// Date + Time plus the assigned RecordIdentifier.
#[derive(
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq,
)]
pub struct StoredRecord {
    pub identifier: RecordIdentifier,
    pub date: Date,
    pub time: Time,
    pub entry: Entry,
}

/// Commands the SEMA actor consumes.
pub enum SemaCommand {
    Record {
        entry: Entry,
        reply: SyncSender<Result<SemaResponse, SemaError>>,
    },
    Observe {
        selection: ObserveSelection,
        reply: SyncSender<Result<SemaResponse, SemaError>>,
    },
    State {
        reply: SyncSender<Result<SemaResponse, SemaError>>,
    },
    Shutdown {
        reply: SyncSender<Result<(), SemaError>>,
    },
}

#[derive(Debug)]
pub enum SemaResponse {
    Recorded(RecordIdentifier),
    Observed(Vec<StoredRecord>),
    State(StateView),
    Topics(Vec<(Topic, u64)>),
}

#[derive(Clone)]
pub struct SemaHandle {
    sender: SyncSender<SemaCommand>,
    path: PathBuf,
}

impl SemaHandle {
    pub fn new(sender: SyncSender<SemaCommand>, path: PathBuf) -> Self {
        Self { sender, path }
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }

    pub fn record(&self, entry: Entry) -> Result<SemaResponse, SemaError> {
        let (reply, receive) = sync_channel(1);
        self.sender
            .send(SemaCommand::Record { entry, reply })
            .map_err(|e| SemaError::Channel(e.to_string()))?;
        receive
            .recv()
            .map_err(|e| SemaError::Channel(e.to_string()))?
    }

    pub fn observe(&self, selection: ObserveSelection) -> Result<SemaResponse, SemaError> {
        let (reply, receive) = sync_channel(1);
        self.sender
            .send(SemaCommand::Observe { selection, reply })
            .map_err(|e| SemaError::Channel(e.to_string()))?;
        receive
            .recv()
            .map_err(|e| SemaError::Channel(e.to_string()))?
    }

    pub fn state(&self) -> Result<SemaResponse, SemaError> {
        let (reply, receive) = sync_channel(1);
        self.sender
            .send(SemaCommand::State { reply })
            .map_err(|e| SemaError::Channel(e.to_string()))?;
        receive
            .recv()
            .map_err(|e| SemaError::Channel(e.to_string()))?
    }

    pub fn shutdown(&self) -> Result<(), SemaError> {
        let (reply, receive) = sync_channel(1);
        self.sender
            .send(SemaCommand::Shutdown { reply })
            .map_err(|e| SemaError::Channel(e.to_string()))?;
        receive
            .recv()
            .map_err(|e| SemaError::Channel(e.to_string()))?
    }
}

/// Schema-version marker — stored in the meta table. The hash comes
/// from the asschema's canonical hash at build time and is baked in
/// via `env!("DESIGN_DEEP_SPIRIT_NEXT_PASS_SCHEMA_HASH")`.
const SCHEMA_VERSION_KEY: &str = "schema_version";

pub struct SemaActor {
    database: Database,
    receiver: Receiver<SemaCommand>,
    next_identifier: u64,
}

impl SemaActor {
    pub fn open(path: impl AsRef<Path>) -> Result<(SemaHandle, JoinHandle<()>), SemaError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let database = Database::create(&path)?;
        Self::bootstrap_or_migrate(&database)?;
        let next_identifier = Self::load_next_identifier(&database)?;
        let (sender, receiver) = sync_channel::<SemaCommand>(64);
        SEMA_ACTOR_COUNT.fetch_add(1, Ordering::SeqCst);
        let actor = Self {
            database,
            receiver,
            next_identifier,
        };
        let join = thread::Builder::new()
            .name("design-deep-spirit-next-pass-sema-actor".to_owned())
            .spawn(move || actor.run())
            .expect("spawn SemaActor thread");
        Ok((SemaHandle::new(sender, path), join))
    }

    fn bootstrap_or_migrate(database: &Database) -> Result<(), SemaError> {
        let schema_hash: &str = env!("DESIGN_DEEP_SPIRIT_NEXT_PASS_SCHEMA_HASH");
        let write = database.begin_write()?;
        {
            let mut meta = write.open_table(META_TABLE)?;
            let existing = meta.get(SCHEMA_VERSION_KEY)?.map(|guard| guard.value().clone());
            match existing {
                None => {
                    meta.insert(
                        SCHEMA_VERSION_KEY,
                        schema_hash.as_bytes().to_vec(),
                    )?;
                }
                Some(bytes) => {
                    let recorded = String::from_utf8_lossy(&bytes).into_owned();
                    if recorded != schema_hash {
                        return Err(SemaError::SchemaVersionMismatch {
                            found: recorded,
                            expected: schema_hash.to_owned(),
                        });
                    }
                }
            }
            let _records = write.open_table(RECORDS_TABLE)?;
        }
        write.commit()?;
        Ok(())
    }

    fn load_next_identifier(database: &Database) -> Result<u64, SemaError> {
        let read = database.begin_read()?;
        let table = match read.open_table(RECORDS_TABLE) {
            Ok(table) => table,
            Err(_) => return Ok(1),
        };
        let highest = table
            .iter()?
            .filter_map(|entry| entry.ok())
            .map(|(key, _)| key.value())
            .max();
        Ok(highest.map(|highest| highest + 1).unwrap_or(1))
    }

    fn run(mut self) {
        while let Ok(command) = self.receiver.recv() {
            match command {
                SemaCommand::Record { entry, reply } => {
                    let response = self.do_record(entry);
                    let _ = reply.send(response);
                }
                SemaCommand::Observe { selection, reply } => {
                    let response = self.do_observe(selection);
                    let _ = reply.send(response);
                }
                SemaCommand::State { reply } => {
                    let response = self.do_state();
                    let _ = reply.send(response);
                }
                SemaCommand::Shutdown { reply } => {
                    let _ = reply.send(Ok(()));
                    break;
                }
            }
        }
        SEMA_ACTOR_COUNT.fetch_sub(1, Ordering::SeqCst);
    }

    fn do_record(&mut self, entry: Entry) -> Result<SemaResponse, SemaError> {
        let identifier = RecordIdentifier(self.next_identifier);
        self.next_identifier += 1;
        let stored = StoredRecord::new_stamped(identifier.clone(), entry);
        let bytes = rkyv::to_bytes::<RkyvError>(&stored)?.to_vec();
        let write = self.database.begin_write()?;
        {
            let mut records = write.open_table(RECORDS_TABLE)?;
            records.insert(identifier.0, bytes)?;
        }
        write.commit()?;
        Ok(SemaResponse::Recorded(identifier))
    }

    fn do_observe(&mut self, selection: ObserveSelection) -> Result<SemaResponse, SemaError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RECORDS_TABLE)?;
        let mut matches: Vec<StoredRecord> = Vec::new();
        let _ = &selection.observation_mode;
        for entry in table.iter()? {
            let (_, value) = entry?;
            let bytes = value.value();
            let stored: StoredRecord = rkyv::from_bytes::<StoredRecord, RkyvError>(&bytes)?;
            if Self::stored_matches(&stored, &selection) {
                matches.push(stored);
            }
        }
        Ok(SemaResponse::Observed(matches))
    }

    fn do_state(&mut self) -> Result<SemaResponse, SemaError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RECORDS_TABLE)?;
        let mut counts: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let bytes = value.value();
            let stored: StoredRecord = rkyv::from_bytes::<StoredRecord, RkyvError>(&bytes)?;
            // TopicList currently models a single topic (one Topic
            // newtype-wrapped); v0.3 vector support is the schema-
            // language gap noted in /374 Q1. We index by that single
            // topic.
            let topic_key = stored.entry.topic_list.0.0.clone();
            *counts.entry(topic_key).or_insert(0) += 1;
        }
        let topics: Vec<(Topic, u64)> = counts
            .into_iter()
            .map(|(name, count)| (Topic(name), count))
            .collect();
        Ok(SemaResponse::Topics(topics))
    }

    fn stored_matches(stored: &StoredRecord, selection: &ObserveSelection) -> bool {
        let topic_ok = match &selection.topic_query {
            TopicQuery::NoTopic => true,
            TopicQuery::TopicMatch(topic) => &stored.entry.topic_list.0 == topic,
        };
        let kind_ok = match &selection.kind_query {
            KindQuery::NoKind => true,
            KindQuery::KindMatch(kind) => &stored.entry.kind == kind,
        };
        topic_ok && kind_ok
    }
}

impl StoredRecord {
    pub fn new_stamped(identifier: RecordIdentifier, entry: Entry) -> Self {
        Self {
            identifier,
            date: DaemonClock::date_now(),
            time: DaemonClock::time_now(),
            entry,
        }
    }
}

/// Daemon-stamped clock — Date is UTC day-start in epoch seconds, Time
/// is full epoch seconds at the record's insert.
pub struct DaemonClock;

impl DaemonClock {
    pub fn date_now() -> Date {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let day_seconds: u64 = 86_400;
        Date(seconds / day_seconds * day_seconds)
    }

    pub fn time_now() -> Time {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        Time(seconds)
    }
}

// Unused-warning suppression for types kept for completeness.
const _: std::marker::PhantomData<(TopicList, TopicCount, TopicCountList, ObservationMode, Magnitude, Description, Kind)> =
    std::marker::PhantomData;
const _: std::marker::PhantomData<StateView> = std::marker::PhantomData;

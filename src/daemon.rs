//! Daemon plane — unix-socket server bridging signal → executor →
//! signal. The dispatch matches schema-emitted `Input` variants per
//! /371; SEMA owns durable storage.

use std::{
    io,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    thread::JoinHandle,
};

use crate::{
    executor::Engine,
    sema::{SemaActor, SemaError},
    signal::{LengthPrefix, TransportError},
};

#[derive(Debug)]
pub enum DaemonError {
    Io(io::Error),
    Sema(SemaError),
    Transport(TransportError),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "daemon io: {error}"),
            Self::Sema(error) => write!(formatter, "daemon sema: {error}"),
            Self::Transport(error) => write!(formatter, "daemon transport: {error}"),
        }
    }
}

impl std::error::Error for DaemonError {}

impl From<io::Error> for DaemonError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<SemaError> for DaemonError {
    fn from(value: SemaError) -> Self {
        Self::Sema(value)
    }
}

impl From<TransportError> for DaemonError {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

/// One-shot run: bind, spawn SEMA, accept until shutdown signal.
pub fn run_daemon(
    socket_path: impl AsRef<Path>,
    database_path: impl AsRef<Path>,
) -> Result<DaemonHandle, DaemonError> {
    let socket_path = socket_path.as_ref().to_path_buf();
    let database_path = database_path.as_ref().to_path_buf();
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }
    let (sema_handle, sema_join) = SemaActor::open(&database_path)?;
    let engine = Engine::new(sema_handle.clone());
    let listener = UnixListener::bind(&socket_path)?;

    let server_socket_path = socket_path.clone();
    let server_join = std::thread::Builder::new()
        .name("design-deep-spirit-next-pass-server".to_owned())
        .spawn(move || {
            let _ = serve(listener, engine);
            let _ = std::fs::remove_file(&server_socket_path);
        })
        .expect("spawn server thread");

    Ok(DaemonHandle {
        socket_path,
        database_path,
        sema_handle,
        sema_join: Some(sema_join),
        server_join: Some(server_join),
    })
}

fn serve(listener: UnixListener, engine: Engine) -> Result<(), DaemonError> {
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(_) => break,
        };
        if let Err(error) = ConnectionHandler.handle(&engine, stream) {
            eprintln!("connection error: {error}");
        }
    }
    Ok(())
}

struct ConnectionHandler;

impl ConnectionHandler {
    fn handle(&self, engine: &Engine, mut stream: UnixStream) -> Result<(), DaemonError> {
        let (_route, input) = LengthPrefix::read_input(&mut stream)?;
        let output = engine.handle(input);
        LengthPrefix::write_output(&mut stream, &output)?;
        Ok(())
    }
}

pub struct DaemonHandle {
    socket_path: PathBuf,
    database_path: PathBuf,
    sema_handle: crate::sema::SemaHandle,
    sema_join: Option<JoinHandle<()>>,
    server_join: Option<JoinHandle<()>>,
}

impl DaemonHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn shutdown(mut self) -> Result<(), DaemonError> {
        let _ = self.sema_handle.shutdown();
        if let Some(join) = self.sema_join.take() {
            let _ = join.join();
        }
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(join) = self.server_join.take() {
            let _ = join.join();
        }
        Ok(())
    }
}

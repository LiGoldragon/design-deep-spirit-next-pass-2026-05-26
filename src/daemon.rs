//! Daemon plane — unix-socket server bridging signal → executor →
//! signal. The dispatch matches schema-emitted `Input` variants per
//! /371; SEMA owns durable storage.

use std::{
    io,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
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
    // Non-blocking accept lets the server loop notice the shutdown
    // flag without an external connect-to-wake-up hack.
    listener.set_nonblocking(true)?;
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let server_socket_path = socket_path.clone();
    let server_flag = shutdown_flag.clone();
    let server_join = std::thread::Builder::new()
        .name("design-deep-spirit-next-pass-server".to_owned())
        .spawn(move || {
            let _ = serve(listener, engine, server_flag);
            let _ = std::fs::remove_file(&server_socket_path);
        })
        .expect("spawn server thread");

    Ok(DaemonHandle {
        socket_path,
        database_path,
        sema_handle,
        sema_join: Some(sema_join),
        server_join: Some(server_join),
        shutdown_flag,
    })
}

fn serve(
    listener: UnixListener,
    engine: Engine,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), DaemonError> {
    while !shutdown_flag.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = ConnectionHandler.handle(&engine, stream) {
                    eprintln!("connection error: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
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
    shutdown_flag: Arc<AtomicBool>,
}

impl DaemonHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn shutdown(mut self) -> Result<(), DaemonError> {
        // Tell the server loop to exit on its next non-blocking poll.
        self.shutdown_flag.store(true, Ordering::SeqCst);
        // Drain SEMA.
        let _ = self.sema_handle.shutdown();
        if let Some(join) = self.sema_join.take() {
            let _ = join.join();
        }
        if let Some(join) = self.server_join.take() {
            let _ = join.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }
}

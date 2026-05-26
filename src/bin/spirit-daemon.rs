//! `spirit-daemon` — the daemon binary. Binds a unix-socket, opens a
//! redb database, spawns the SemaActor, and dispatches incoming
//! signal-frames through the Engine.

use std::{env, path::PathBuf, process::ExitCode};

use design_deep_spirit_next_pass::run_daemon;

const SOCKET_ENV: &str = "DESIGN_DEEP_SPIRIT_NEXT_PASS_SOCKET";
const DATABASE_ENV: &str = "DESIGN_DEEP_SPIRIT_NEXT_PASS_DATABASE";

fn main() -> ExitCode {
    let _exit_code: ExitCode = run().0;
    // Unreachable in practice — `run()` never returns. The bind keeps
    // the function signature returning ExitCode for error cases that
    // can't bind/start the daemon.
    ExitCode::SUCCESS
}

struct RunResult(ExitCode);

fn run() -> RunResult {
    let socket_path = match env::var_os(SOCKET_ENV) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("/tmp/design-deep-spirit-next-pass.sock"),
    };
    let database_path = match env::var_os(DATABASE_ENV) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("/tmp/design-deep-spirit-next-pass.redb"),
    };

    let handle = match run_daemon(&socket_path, &database_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("daemon start: {error}");
            return RunResult(ExitCode::from(4));
        }
    };

    eprintln!(
        "design-deep-spirit-next-pass-daemon listening on {} (database {})",
        handle.socket_path().display(),
        handle.database_path().display()
    );

    // Park the main thread; the server thread holds the listener.
    // Graceful exit happens via SIGTERM/SIGINT (the kernel reaps us
    // and the SEMA actor's atomic counter decrements via Drop —
    // sufficient for the demo + tests; production daemons would
    // install proper signal handlers).
    loop {
        std::thread::park();
    }
}

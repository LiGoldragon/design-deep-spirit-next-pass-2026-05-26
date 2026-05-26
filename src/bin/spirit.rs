//! `spirit` — the thin CLI. Reads a NOTA input from argv (the
//! single-argument rule per skills/component-triad.md). Parses it
//! through the emitted `Input::from_str` (NOTA codec). Sends via
//! length-prefix + signal-frame to the daemon. Prints the daemon's
//! reply as NOTA on stdout.

use std::{env, path::PathBuf, process::ExitCode, str::FromStr};

use design_deep_spirit_next_pass::{ExchangeClient, Input, Output};

const SOCKET_ENV: &str = "DESIGN_DEEP_SPIRIT_NEXT_PASS_SOCKET";

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let argument = match arguments.next() {
        Some(value) => value,
        None => {
            eprintln!(
                "{} accepts NOTA or signal-file input, not flag-style argument",
                env::current_exe()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "spirit".to_owned())
            );
            return ExitCode::from(2);
        }
    };

    let input_source = if argument.starts_with('(') {
        argument
    } else {
        match std::fs::read_to_string(&argument) {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("read input file {argument}: {error}");
                return ExitCode::from(2);
            }
        }
    };

    let input = match Input::from_str(&input_source) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("parse input: {error}");
            return ExitCode::from(3);
        }
    };

    let socket_path = match env::var_os(SOCKET_ENV) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("/tmp/design-deep-spirit-next-pass.sock"),
    };

    let (_route, output) = match ExchangeClient::exchange(&socket_path, &input) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("daemon exchange: {error}");
            return ExitCode::from(4);
        }
    };

    println!("{}", OutputDisplay::new(&output));
    ExitCode::SUCCESS
}

struct OutputDisplay<'a> {
    output: &'a Output,
}

impl<'a> OutputDisplay<'a> {
    fn new(output: &'a Output) -> Self {
        Self { output }
    }
}

impl std::fmt::Display for OutputDisplay<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.output.to_nota())
    }
}

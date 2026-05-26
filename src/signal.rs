//! Signal plane — thin length-prefix shim over schema-emitted codec.
//!
//! Per designer/371 the signal plane owns wire framing + short-header
//! triage. Everything route/header/codec-shaped comes from the emitted
//! methods on `Input` / `Output`. This module only wraps the outer
//! length envelope for the Unix-socket transport.
//!
//! The route/header/codec methods exist on `Input` + `Output` because
//! the schema's `(Route Input)` / `(Route Output)` macro CALLS produced
//! corresponding `InputRoute` + `OutputRoute` enums in the lowered
//! asschema's namespace; the emitter then attached the route + codec
//! methods to each surface that has a matching `<Surface>Route` enum.

use std::{
    fmt,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
};

use crate::{Input, InputRoute, Output, OutputRoute, SignalFrameError};

const LENGTH_PREFIX_BYTE_COUNT: usize = 4;

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    SignalFrame(SignalFrameError),
    FrameTooLarge { found: usize },
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "transport IO: {error}"),
            Self::SignalFrame(error) => write!(formatter, "signal-frame: {error}"),
            Self::FrameTooLarge { found } => {
                write!(formatter, "frame too large for u32 prefix: {found} bytes")
            }
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<SignalFrameError> for TransportError {
    fn from(value: SignalFrameError) -> Self {
        Self::SignalFrame(value)
    }
}

/// Single round-trip — connect, send input, read reply.
pub struct ExchangeClient;

impl ExchangeClient {
    pub fn exchange(
        socket_path: impl AsRef<Path>,
        input: &Input,
    ) -> Result<(OutputRoute, Output), TransportError> {
        let mut stream = UnixStream::connect(socket_path)?;
        LengthPrefix::write_input(&mut stream, input)?;
        LengthPrefix::read_output(&mut stream)
    }
}

/// Length-prefix codec — outer wire envelope around the emitted
/// signal-frame.
pub struct LengthPrefix;

impl LengthPrefix {
    pub fn write_input(writer: &mut impl Write, input: &Input) -> Result<(), TransportError> {
        let frame = input.encode_signal_frame()?;
        Self::write_envelope(writer, &frame)
    }

    pub fn read_input(reader: &mut impl Read) -> Result<(InputRoute, Input), TransportError> {
        let frame = Self::read_envelope(reader)?;
        Input::decode_signal_frame(&frame).map_err(TransportError::SignalFrame)
    }

    pub fn write_output(writer: &mut impl Write, output: &Output) -> Result<(), TransportError> {
        let frame = output.encode_signal_frame()?;
        Self::write_envelope(writer, &frame)
    }

    pub fn read_output(reader: &mut impl Read) -> Result<(OutputRoute, Output), TransportError> {
        let frame = Self::read_envelope(reader)?;
        Output::decode_signal_frame(&frame).map_err(TransportError::SignalFrame)
    }

    fn write_envelope(writer: &mut impl Write, frame: &[u8]) -> Result<(), TransportError> {
        let length = u32::try_from(frame.len())
            .map_err(|_| TransportError::FrameTooLarge { found: frame.len() })?;
        writer.write_all(&length.to_be_bytes())?;
        writer.write_all(frame)?;
        writer.flush()?;
        Ok(())
    }

    fn read_envelope(reader: &mut impl Read) -> Result<Vec<u8>, TransportError> {
        let mut length_bytes = [0_u8; LENGTH_PREFIX_BYTE_COUNT];
        reader.read_exact(&mut length_bytes)?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        let mut frame = vec![0_u8; length];
        reader.read_exact(&mut frame)?;
        Ok(frame)
    }
}

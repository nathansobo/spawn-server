extern crate bytes;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;

use std::process::{Command, ExitStatus, Stdio};
use std::io;

use bytes::{BytesMut};
use futures::{Future, Stream};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_io::codec::{Decoder, Encoder, FramedRead, FramedWrite};
use tokio_process::CommandExt;

struct SpawnCodec;

impl Encoder for SpawnCodec {
    type Item = SpawnResult;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut) -> io::Result<()> {
        match msg {
            SpawnResult::ChildOutput { source, data } => {
                buf.extend(source.to_string().as_bytes());
                buf.extend(b": ");
                buf.extend(data);
                buf.extend(b"\n")
            },
            SpawnResult::ChildExit { status } => {
                buf.extend(b"exit: ");
                buf.extend(status.code().unwrap().to_string().as_bytes());
                buf.extend(b"\n");
            }
        }

        Ok(())
    }
}

enum SpawnResult {
    ChildOutput {
        source: OutputStreamType,
        data: BytesMut
    },
    ChildExit {
        status: ExitStatus
    }
}

#[derive(Clone, Copy)]
enum OutputStreamType {
    Stdout,
    Stderr
}

impl ToString for OutputStreamType {
    fn to_string(&self) -> String {
        match *self {
            OutputStreamType::Stdout => String::from("stdout"),
            OutputStreamType::Stderr => String::from("stderr")
        }
    }
}

struct ChildOutputStreamDecoder {
    source: OutputStreamType
}

impl ChildOutputStreamDecoder {
    pub fn from_stdout() -> Self {
        Self {
            source: OutputStreamType::Stdout
        }
    }

    pub fn from_stderr() -> Self {
        Self {
            source: OutputStreamType::Stderr
        }
    }
}

impl Decoder for ChildOutputStreamDecoder {
    type Item = SpawnResult;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<SpawnResult>> {
        if buf.len() > 0 {
            Ok(Some(SpawnResult::ChildOutput {
                source: self.source,
                data: buf.take()
            }))
        } else {
            Ok(None)
        }
    }
}

fn main() {
    // Build the event loop and get a handle to it.
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    // Create a TCP listener on the event loop. Later we'll use a domain socket / named pipe.
    let address = "0.0.0.0:12345".parse().unwrap();
    let listener = TcpListener::bind(&address, &handle).unwrap();

    // Handle a stream of incoming connections.
    let handle_connections = listener.incoming().for_each(move |(tcp_stream, _)| {
        let transport = FramedWrite::new(tcp_stream, SpawnCodec);

        let mut child = Command::new("sh")
            .arg("-c")
            .arg("echo hello")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn_async(&handle)
            .expect("Failed to execute sh");

        let stdout = FramedRead::new(child.stdout().take().unwrap(), ChildOutputStreamDecoder::from_stdout());
        let stderr = FramedRead::new(child.stderr().take().unwrap(), ChildOutputStreamDecoder::from_stderr());
        let exit = child.map(|status| SpawnResult::ChildExit { status }).into_stream();

        let respond = stdout
            .select(stderr)
            .chain(exit)
            .forward(transport)
            .then(|_| Ok(()));

        handle.spawn(respond);
        Ok(())
    });

    // Handle incoming connections on the event loop.
    core.run(handle_connections).unwrap();
}

#[macro_use]
extern crate serde_derive;

extern crate bytes;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
extern crate serde;
extern crate serde_json;

use std::error::Error;
use std::process::{Command, ExitStatus, Stdio};
use std::io;

use bytes::BytesMut;
use futures::{Future, Sink, Stream};
use futures::unsync::mpsc::unbounded;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_io::AsyncRead;
use tokio_io::codec::{Decoder, Encoder, FramedRead};
use tokio_process::CommandExt;

struct SpawnCodec;

impl Decoder for SpawnCodec {
    type Item = SpawnRequest;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.len() == 0 {
            return Ok(None);
        }

        match serde_json::from_slice(buf.as_ref()) {
            Ok(result) => {
                buf.take();
                println!("PARSED");
                Ok(Some(result))
            },
            Err(error) => {
                buf.take();
                println!("PARSE ERROR");
                if error.is_eof() {
                    Ok(None)
                } else {
                    Ok(Some(SpawnRequest {path: String::from(""), args: vec![]}))
                    // Err(io::Error::new(io::ErrorKind::InvalidInput, error.description()))
                }
            }
        }
    }
}

impl Encoder for SpawnCodec {
    type Item = SpawnResponse;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut) -> io::Result<()> {
        match msg {
            SpawnResponse::ChildOutput { source, data } => {
                buf.extend(source.to_string().as_bytes());
                buf.extend(b": ");
                buf.extend(data);
                buf.extend(b"\n")
            },
            SpawnResponse::ChildExit { status } => {
                buf.extend(b"exit: ");
                buf.extend(status.code().unwrap().to_string().as_bytes());
                buf.extend(b"\n");
            }
        }

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct SpawnRequest {
    path: String,
    args: Vec<String>
}

enum SpawnResponse {
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
    type Item = SpawnResponse;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<SpawnResponse>> {
        if buf.len() > 0 {
            Ok(Some(SpawnResponse::ChildOutput {
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
        println!("HANDLE CONNECTION");

        let (writer, reader) = tcp_stream.framed(SpawnCodec).split();

        let (tx, rx) = unbounded::<SpawnResponse>();

        let handle_clone = handle.clone();
        let respond = reader.for_each(move |request| {
            println!("REQUEST");

            let mut child = Command::new("sh")
                .arg("-c")
                .arg("echo hello")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn_async(&handle_clone)
                .expect("Failed to execute sh");

            let stdout = FramedRead::new(child.stdout().take().unwrap(), ChildOutputStreamDecoder::from_stdout());
            let stderr = FramedRead::new(child.stderr().take().unwrap(), ChildOutputStreamDecoder::from_stderr());
            let exit = child.map(|status| SpawnResponse::ChildExit { status }).into_stream();
            let child_results = stdout.select(stderr).chain(exit);

            let send = tx.clone()
                .send_all(child_results.map_err(|_| unreachable!()))
                .then(|_| Ok(()));

            &handle_clone.spawn(send);
            Ok(())
        });

        handle.spawn(writer.send_all(rx.map_err(|_| io::Error::new(io::ErrorKind::Other, "Problem"))).then(|_| Ok(())));

        handle.spawn(respond.then(|_| Ok(())));
        Ok(())
    });

    // Handle incoming connections on the event loop.
    core.run(handle_connections).unwrap();
}

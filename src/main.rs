#[macro_use]
extern crate serde_derive;

extern crate bytes;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
extern crate serde;
extern crate serde_json;

use std::process::{Command, ExitStatus, Stdio};
use std::fmt::Debug;
use std::io;

use bytes::BytesMut;
use futures::{Async, Future, Poll, Sink};
use futures::stream::{FuturesUnordered, Stream, StreamFuture};
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

#[derive(Debug)]
enum SpawnResponse {
    ChildOutput {
        source: OutputStreamType,
        data: BytesMut
    },
    ChildExit {
        status: ExitStatus
    }
}

#[derive(Clone, Copy, Debug)]
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

    let address = "0.0.0.0:12345".parse().unwrap();
    let listener = TcpListener::bind(&address, &handle).unwrap();

    let handle_connections = listener.incoming().for_each(move |(tcp_stream, _)| {
        let (responses_sink, requests_stream) = tcp_stream.framed(SpawnCodec).split();

        let handle_for_request = handle.clone();
        let responses = MergeResponseStreams::new(requests_stream.map(move |request| {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg("echo hello")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn_async(&handle_for_request)
                .expect("Failed to execute sh");

            let stdout = FramedRead::new(child.stdout().take().unwrap(), ChildOutputStreamDecoder::from_stdout());
            let stderr = FramedRead::new(child.stderr().take().unwrap(), ChildOutputStreamDecoder::from_stderr());
            let exit = child.map(|status| SpawnResponse::ChildExit { status }).into_stream();
            stdout.select(stderr).chain(exit)
        }));

        handle.spawn(responses_sink.send_all(responses).then(|_| Ok(())));
        Ok(())
    });

    // Handle incoming connections on the event loop.
    core.run(handle_connections).unwrap();
}

struct MergeResponseStreams<S>
    where S: Stream, S::Item: Stream
{
    parent_stream: S,
    child_stream_futures: FuturesUnordered<StreamFuture<S::Item>>
}

impl<S> MergeResponseStreams<S>
    where S: Stream, <S as Stream>::Item: Stream
{
    fn new(parent_stream: S) -> Self {
        Self {
            parent_stream,
            child_stream_futures: FuturesUnordered::new()
        }
    }
}

impl<S> Stream for MergeResponseStreams<S>
    where
        S: Stream, S::Item: Stream,
        <S::Item as Stream>::Error: Debug,
        <S::Item as Stream>::Item: Debug
{
    type Item = <S::Item as Stream>::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.parent_stream.poll()? {
            Async::NotReady => {
                if self.child_stream_futures.is_empty() {
                    return Ok(Async::NotReady);
                }
            }
            Async::Ready(Some(child_stream)) => {
                self.child_stream_futures.push(child_stream.into_future())
            }
            Async::Ready(None) => {
                if self.child_stream_futures.is_empty() {
                    return Ok(Async::Ready(None));
                }
            }
        }

        match self.child_stream_futures
            .poll()
            .map_err(|(err, _)| err)
            .expect("Child streams should not produce errors")
        {
            Async::Ready(Some((Some(response), child_stream))) => {
                self.child_stream_futures.push(child_stream.into_future());
                Ok(Async::Ready(Some(response)))
            }
            _ => {
                Ok(Async::NotReady)
            }
        }
    }
}

extern crate serde;
extern crate serde_json;

use std::io;
use std::process::ExitStatus;

use bytes::BytesMut;
use tokio_io::codec::{Decoder, Encoder};

#[derive(Debug, Deserialize)]
pub struct SpawnRequest {
    path: String,
    args: Vec<String>
}

#[derive(Debug)]
pub enum SpawnResponse {
    ChildOutput {
        source: OutputStreamType,
        data: BytesMut
    },
    ChildExit {
        status: ExitStatus
    }
}

#[derive(Clone, Copy, Debug)]
pub enum OutputStreamType {
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

pub struct SpawnCodec;

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

pub struct ChildOutputStreamDecoder {
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

extern crate serde;
extern crate serde_json;

use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::process::ExitStatus;

use bytes::{BytesMut, BigEndian};
use bytes::buf::BufMut;
use tokio_io::codec::{Decoder, Encoder};

#[derive(Debug, Deserialize)]
pub struct SpawnRequest {
    pub path: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: HashMap<String, String>
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
                Ok(Some(result))
            },
            Err(error) => {
                buf.take();
                if error.is_eof() {
                    Ok(None)
                } else {
                    Err(io::Error::new(io::ErrorKind::InvalidInput, error.description()))
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
                match source {
                    OutputStreamType::Stdout => buf.put_u8(1 << 0),
                    OutputStreamType::Stderr => buf.put_u8(1 << 1),
                }
                buf.put_u64::<BigEndian>(data.len() as u64);
                buf.extend(data);
            },
            SpawnResponse::ChildExit { status } => {
                buf.put_u8(0);
                buf.put_i32::<BigEndian>(status.code().unwrap());
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

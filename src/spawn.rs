use std::process::{Command, Stdio};
use std::fmt::Debug;

use futures::{Async, Future, Poll};
use futures::stream::{FuturesUnordered, Stream, StreamFuture};
use tokio_core::reactor::{Handle};
use tokio_io::codec::{FramedRead};
use tokio_process::CommandExt;

use codecs::*;

pub fn handle_spawn_requests<'a, S>(requests: S, handle: Handle) -> Box<'a + Stream<Item=SpawnResponse, Error=S::Error>>
    where S: 'a + Stream<Item=SpawnRequest>,
{
    Box::new(MergeResponseStreams::new(requests.map(move |request| {
        let request_id = request.id;

        let mut child = Command::new(request.path)
            .args(request.args)
            .current_dir(request.cwd)
            .envs(request.env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn_async(&handle)
            .expect("Failed to execute sh");

        let stdout = FramedRead::new(child.stdout().take().unwrap(), ChildOutputStreamDecoder::from_stdout(request_id));
        let stderr = FramedRead::new(child.stderr().take().unwrap(), ChildOutputStreamDecoder::from_stderr(request_id));
        let exit = child.map(move |status| SpawnResponse::ChildExit { request_id, status }).into_stream();
        stdout.select(stderr).chain(exit)
    })))
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

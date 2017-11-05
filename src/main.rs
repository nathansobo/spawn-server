extern crate bytes;
extern crate futures;
extern crate tokio_core;
extern crate tokio_process;
extern crate tokio_io;

use std::process::{Command, Stdio};

use futures::{Future, Stream};
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_process::{CommandExt};

fn main() {
    // Build the event loop
    let mut core = Core::new().unwrap();
    // Create a handle that we can use to schedule promises on the event loop
    let handle = core.handle();

    // Create a TCP listener on the event loop.
    // This can be replaced with a domain socket / named pipe listener later.
    let address = "0.0.0.0:12345".parse().unwrap();
    let listener = TcpListener::bind(&address, &handle).unwrap();

    // Create a future based on handling a stream of incoming connections.
    let handle_connections = listener.incoming().for_each(move |(tcp_stream, _)| {
        // For each connection, spawn a subprocess on the event loop.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("echo hello")
            .stdout(Stdio::piped())
            .spawn_async(&handle)
            .expect("Failed to execute sh");

        // Copy the bytes from the stdout of the child process to the tcp stream and wait for the
        // child process to exit.
        let stdout = child.stdout().take().expect("Stdout!");
        let respond =
            tokio_io::io::copy(stdout, tcp_stream)
            .join(child)
            .then(|_| Ok(()));

        // Schedule the response on the event loop.
        handle.spawn(respond);
        Ok(())
    });

    // Handle incoming connections on the event loop.
    core.run(handle_connections).unwrap();
}

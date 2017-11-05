# Spawn server

For [complicated reasons](https://github.com/nodejs/node/issues/14917), spawning subprocesses in Electron applications can block the event loop and impact application responsiveness.

The goal of this repository is to create a lightweight, high-performance server that Electron apps can use to spawn subprocesses on their behalf.

* The server will listen on a domain socket.
* When the client needs to spawn a subprocess, it will connect to the server via the domain socket and send the server JSON with a command `path`, an `args` array, and an `env` object.
* The server will spawn the subprocess and stream chunks of `stdout` and `stderr` back to the client as they become available. When the process terminates, it will send back the exit code.

The goal is for the server to be a native executable, have a minimal memory footprint, and be completely event driven so it only consumes a single thread.

Currently, I have a toy example started in Rust that uses the [tokio framework](https://tokio.rs/) for asynchronous I/O. It currently listens on a TCP socket and just runs `sh -c echo hello` for every connection, streaming stdout back to the client.

## Next steps:

* Replace the hard-coded command with a JSON request supplied by the connection.
* Stream both stdout and stderr back to the client with some sort of framing scheme that indicates which stream a given chunk of data belongs to.
* Send the result code on process termination before closing the connection.
* Listen on domain sockets on Unix, named pipes on Windows.
* Write a client implementation in JS.

## Maybe someday?

* Multi-plexed communication so that multiple subprocesses can be managed over the same connection
* The ability to kill subprocesses from the client

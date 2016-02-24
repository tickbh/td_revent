# Event - Async IO

Event is a lightweight IO library for Rust with a focus on adding as
little overhead as possible over the OS abstractions.

**Getting started guide**
Currently a work in progress:

## Usage

To use `event_rust`, first add this to your `Cargo.toml`:

```toml
[dependencies]
event_rust = "0.1.0"
```

Then, add this to your crate root:

```rust
extern crate event;
```

## Features

* Event loop backed by epoll, windows by select.
* Non-blocking TCP sockets
* High performance timer system

## Platforms

Currently, event_rust only supports Linux and Windows. The goal is to support
all platforms that support Rust and the readiness IO model.

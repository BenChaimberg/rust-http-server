A HTTP server implemented in Rust, available in single- and multi-threaded variants.

# Design

The directory structure below may serve to give a high-level overview of the design of the server. In general, the goal is to separate concernes, where most of the implementation should have no connection to concepts of threading or streams. Serialization should be done in one location and various multi-threading techniques should be able to use the same backend HTTP request processing.

Beyond the general file structure, some details related to multi-threaded processing can be described. For thread pool processing, a single thread repeatedly waits for an incoming connection. When such a connection arrives, a thread is popped off a ready list of threads and tasked with processing the newly accepted connection. When the thread is done, it signals that it is ready and waits until a new connection is given. This is remarkably easy to feel confident about given Rust message-passing semantics.

For selector IO multiplexing, an event loop repeatedly polls all registered event sources and sequentially processes all returned events. Each event is associated with a handler (stored in a hash map with a unique token as a key) that is used to process the event on that source. The handler may return a command, which will be executed during the asynchronous command processing phase of the event loop. These commands can be submitted at any time but will only be run during an iteration of the event loop. Commands can have arbitrary body and can also return certain information that will allow the event loop to perform IO-related tasks on behalf of the command, such as registering a new source.

Request parsing and response writing in the selector multiplexing model is more iterative than the sequential and thread pool models. Reading and writing from streams is done iteratively and requires distinct data structures that hold partially read requests and written responses.

Heartbeat monitoring is implemented a refreshingly simple fashion for the sequential and select multiplexing models: if the server can respond to the request, then the server is not overloaded. For the thread pool model, information related to the current load is passed to the thread along with the accepted stream. This allows individual threads to determine overloading if such a request is given by the client. The mechanism by which load is calculated is whether there are any existing ready threads when the load request is accepted.

# Compliance

Please note that this server only implements a *small* fraction of the HTTP specification, particularly when it comes to recognizing and abiding by HTTP headers. Other features that are lacking are HTTP methods other than `GET` or `POST`, responsible chunked encoding of ongoing requests (currently only chunks data when the response body is above a certain side). This is obviously not a desired state, but some lacking features are not necessarily required fo a functional HTTP server. In particular, ignoring HTTP headers that are not supported by the serner is a less-than-ideal process, but by the nature of the HTTP specification, the minimal number of headers should be all that is required.

# Directory Structure

```
httpd.conf
src/
├─ cgi.rs
├─ config.rs
├─ error.rs
├─ files.rs
├─ host.rs
├─ http.rs
├─ main.rs
├─ parse.rs
├─ pool.rs
├─ select.rs
├─ seq.rs
├─ time.rs
www/
├─ cgi-bin/
│  ├─ printenv.pl
│  ├─ uppercase.pl
├─ index.html
├─ nested/
│  ├─ index.html
```

## src/

Contains Rust files that can be compiled to produce the server binary. Build and run with `cargo run </path/to/config> <single|pool|select>`.

### cgi.rs

Implements the CGI protocol based on [RFC3875](https://datatracker.ietf.org/doc/html/rfc3875). Supports a subset of the request meta-variables (`QUERY_STRING`, `REMOTE_ADDR`, `REQUEST_METHOD`, `SERVER_NAME`, `SERVER_PORT`, `SERVER_PROTOCOL`, `SERVER_SOFTWARE`). Currently, Fast CGI is not supported.

### config.rs

Parses a configuration file written in the style of the [Apache HTTP Server](https://httpd.apache.org/docs/2.4/configuring.html). The only supported scope is `VirtualHost`. Supports a subset of the directives (`Listen`, `CacheSize`, `DocumentRoot`, `ServerName`).

### error.rs

Defines some error types for shared use throughout the project.

### files.rs

Provides access to static files. Caches the content of the files up to a configurable limit. Does not return data if the resource has not been modified and the requset asks for a cached copy.

### host.rs

Processes requests and produces responses. Currently, representation selection through the `Accept-*` header is not supported.

### http.rs

Communicates with the remote over a TCP socket. Specifically: deserializes requests and serializes responses.

### main.rs

Entry-point into the server. Loads configuration, opens a listening connection, and passes further connection management to single- and multi-threaded variants, below.

### parse.rs

Contains some small utilities for string parsing.

### pool.rs

An implementation for multi-threaded connection processing using producer/consumer model. Each thread uses the connection processing in `seq.rs`.

### select.rs

An implementation for selector IO multiplexing connection processing.

### seq.rs

A single-threaded implementation for connection processing.

### time.rs

Some helper utilities for parsing and serializing times in a specific format (RFC 112)3.

## www/

Contains example files that can be used to test the server.

## httpd.conf

Contains an example configuration file that can be used to test the server. Note that all locations in this file must be absolute paths.

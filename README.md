A single-threaded HTTP server implemented in Rust.

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
www/
├─ cgi-bin/
│  ├─ printenv.pl
│  ├─ uppercase.pl
├─ index.html
├─ nested/
│  ├─ index.html
```

## src/

Contains Rust files that can be compiled to produce the server binary. Build and run with `cargo run`.

### cgi.rs

Implements the CGI protocol based on [RFC3875](https://datatracker.ietf.org/doc/html/rfc3875). Supports a subset of the request meta-variables (`QUERY_STRING`, `REMOTE_ADDR`, `REQUEST_METHOD`, `SERVER_NAME`, `SERVER_PORT`, `SERVER_PROTOCOL`, `SERVER_SOFTWARE`). Currently, Fast CGI is not supported.

### config.rs

Parses a configuration file written in the style of the [Apache HTTP Server](https://httpd.apache.org/docs/2.4/configuring.html). The only supported scope is `VirtualHost`. Supports a subset of the directives (`Listen`, `CacheSize`, `DocumentRoot`, `ServerName`).

### error.rs

Defines some error types for shared use throughout the project.

### files.rs

Provides access to static files. Caches the content of the files up to a configurable limit. Currently, the `If-Modified-Since` header is not respected and the cached content is not refreshed if the file has been modified since it has been cached.

### host.rs

Processes requests and produces responses. Currently, representation selection through the `User-Agent` or `Accept-*` headers is not supported.

### http.rs

Communicates with the remote over a TCP socket. Specifically: deserializes requests, passes to the host, and serializes the response.

### main.rs

Entry-point into the server. Loads configuration, initializes a host, opens a listening socket, and passes accepted connections to the handler.

## www/

Contains example files that can be used to test the server.

## httpd.conf

Contains an example configuration file that can be used to test the server.

---
title: HTTP
summary: std.http: requests and responses, client calls, servers, limits and JSON bodies.
modules: [http]
---

# HTTP

## Overview

`import std.http;` gives an HTTP/1.1 client and server, over plain TCP or
TLS (`https://`). Every function that connects or listens takes the `Net`
first, so HTTP goes only where the program's network allows. A server
calls a **handler**, a function from `Request` to `Response`, in a task of
its own for each connection.

## Reference

### Request and Response

| Item | Meaning |
| --- | --- |
| `Request` | `method`, `path` (before `?`, not decoded), `query` (after `?`, raw), `headers` (`Map`, names in lower case), `body` (`u8[]`), `peer` |
| `Response` | `status`, `headers`, `body` |
| `r.text()` | the body as UTF-8: `Ok(text)` or `Err` |
| `r.header(name)` | a header's value, by name in any case: `Option<string>` |
| `r.json()` | the body parsed as JSON: `Ok(json.Value)` or `Err(JsonError)` |
| `http.text(status, text)` | a `text/plain` response |
| `http.json(status, value)` | an `application/json` response of a `json.Value` |
| `http.bytes(status, body)` | an `application/octet-stream` response |

### Client

| Item | What it does |
| --- | --- |
| `http.get(net, url)` | `GET url`: `Ok(Response)` for any status, `Err(reason)` when no response came |
| `http.post(net, url, content_type, body)` | `POST` with a body |
| `http.post_json(net, url, value)` | `POST` with a `json.Value` as the body |
| `http.request(net, method, url, headers, body)` | any method, with headers |
| `http.request_with(…, limits)` | the same, with other `Limits` |
| `http.request_tls(…, opts)`, `http.request_tls_with(…, limits, opts)` | for `https://` with a `tls.Options`: a private CA, a timeout |

URLs are `http://host[:port]/path?query` or `https://…`; `https://` checks
the server's certificate against the system's trusted roots.

### Server

| Item | What it does |
| --- | --- |
| `Handler` | the handler's type: `fun(Request): Response`, sendable |
| `http.serve(net, addr, handler)` | serves forever; returns `Err` only when it can't listen |
| `http.serve_with(net, addr, limits, handler)` | the same, with other limits |
| `http.serve_tls(net, addr, cert_pem, key_pem, handler)` | serves `https://` with a certificate and key (PEM text) |
| `http.serve_tls_with(…, limits, handler)` | the same, with other limits |
| `http.limits()` | the default `Limits` |
| `Limits` | `line` (8 192 bytes: longer is 414), `headers` (8 192: 431), `header_count` (100: 431), `body` (1 MiB: 413), `timeout_ms` (30 000) |

A handler that panics fails only its own request, with a 500; a request
over a limit gets its status and the connection is closed.

## Examples

```lugha
import std.http;
import std.json;
import std.task;

fun handle(req: http.Request): http.Response = match req.path {
    "/hello" => http.text(200, "hello, {req.query}"),
    "/sum" => match req.json() {
        .Ok(v) => {
            let xs = json.as_array(v).unwrap_or([]);
            let total = xs.iter().map(fun(x) = json.as_i64(x).unwrap_or(0)).sum();
            http.json(200, json.Value.Object(Map { "total": json.Value.Integer(total) }))
        }
        .Err(e) => http.text(400, e.message()),
    },
    _ => http.text(404, "no page {req.path}"),
};

fun get_when_up(net: Net, url: string): http.Response {
    loop {
        match http.get(net, url) {
            .Ok(r) => {
                return r;
            }
            .Err(_) => task.sleep(5),
        }
    }
}

fun run_server(net: Net, addr: string): Result<(), string> = http.serve(net, addr, handle);

fun main(console: Console, net: Net) {
    let base = "http://127.0.0.1:47321";
    let done = scope s {
        let server = s.spawn(run_server(net, "127.0.0.1:47321"));
        let a = get_when_up(net, "{base}/hello?ann");
        console.println("{a.status} {a.text().unwrap()} {a.header("Content-Type").unwrap()}");
        let body = json.parse("[1, 2, 3]").unwrap();
        let b = http.post_json(net, "{base}/sum", body).unwrap();
        console.println("{b.status} {b.text().unwrap()}");
        let c = http.get(net, "{base}/nope").unwrap();
        console.println("{c.status} {c.text().unwrap()}");
        server.cancel("done");
    };
    console.println(done.is_ok());
}
```
```output
200 hello, ann text/plain; charset=utf-8
200 {"total":6}
404 no page /nope
true
```

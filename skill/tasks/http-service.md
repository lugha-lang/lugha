---
title: An HTTP service
summary: A JSON API with std.http: routing, decoding bodies into your types, and errors as statuses.
uses: [structs-and-enums, errors, interfaces, http, json-and-toml]
---

# An HTTP service

## Goal

Build a small JSON API that converts temperatures: `POST /convert` with
`{"value": 100, "to": "F"}` answers `{"value": 212.0, "unit": "F"}`, a bad
body is `400` with the reason, and an unknown path `404`. The handler is a
plain function from request to response, so it is easy to test without a
network.

## Steps

1. **Describe the bodies as types,** and derive `json.Json` so they convert
   to and from JSON.

```lugha part
struct Convert { value: f64, to: string } with json.Json
struct Converted { value: f64, unit: string } with json.Json
```

2. **Turn errors into statuses in one place.** The logic returns
   `Result<Response, Response>`, and the handler takes whichever it got.

```lugha part
fun handle(req: http.Request): http.Response = match route(req) {
    .Ok(r) => r,
    .Err(r) => r,
};
```

3. **Decode the body with `?`,** mapping each error to a `400` response.

```lugha part
    let body = req.json().map_err(fun(e) = http.text(400, e.message()))?;
    let c: Convert = json.decode(body).map_err(fun(e) = http.text(400, e.message()))?;
```

4. **Serve it.** `http.serve` gives each connection its own task and calls
   the handler for each request.

```lugha part
        let server = s.spawn(http.serve(net, "127.0.0.1:47911", handle));
```

## The whole program

The program calls its own service as a client, then stops it, so it runs
anywhere.

```lugha
import std.http;
import std.json;
import std.task;

struct Convert { value: f64, to: string } with json.Json
struct Converted { value: f64, unit: string } with json.Json

fun convert(c: Convert): Result<Converted, string> = match c.to {
    "F" => Ok(Converted { value: c.value * 9.0 / 5.0 + 32.0, unit: "F" }),
    "C" => Ok(Converted { value: (c.value - 32.0) * 5.0 / 9.0, unit: "C" }),
    other => Err("unknown unit {other}: use C or F"),
};

fun route(req: http.Request): Result<http.Response, http.Response> {
    if req.path != "/convert" {
        return Err(http.text(404, "no route {req.path}"));
    }
    if req.method != "POST" {
        return Err(http.text(405, "use POST"));
    }
    let body = req.json().map_err(fun(e) = http.text(400, e.message()))?;
    let c: Convert = json.decode(body).map_err(fun(e) = http.text(400, e.message()))?;
    let out = convert(c).map_err(fun(e) = http.text(400, e))?;
    Ok(http.json(200, json.encode(out)))
}

fun handle(req: http.Request): http.Response = match route(req) {
    .Ok(r) => r,
    .Err(r) => r,
};

fun post(net: Net, url: string, body: string): string {
    loop {
        match http.post(net, url, "application/json", body.to_bytes()) {
            .Ok(r) => {
                return "{r.status} {r.text().unwrap()}";
            }
            // The server task may not be listening yet.
            .Err(_) => task.sleep(5),
        }
    }
}

fun main(console: Console, net: Net) {
    let url = "http://127.0.0.1:47911";
    let r = scope s {
        let server = s.spawn(http.serve(net, "127.0.0.1:47911", handle));
        console.println(post(net, "{url}/convert", "{{\"value\": 100, \"to\": \"F\"}}"));
        console.println(post(net, "{url}/convert", "{{\"value\": 212, \"to\": \"C\"}}"));
        console.println(post(net, "{url}/convert", "{{\"value\": 1, \"to\": \"K\"}}"));
        console.println(post(net, "{url}/convert", "not json"));
        console.println(post(net, "{url}/other", ""));
        server.cancel("done");
    };
}
```
```output
200 {"value":212.0,"unit":"F"}
200 {"value":100.0,"unit":"C"}
400 unknown unit K: use C or F
400 expected a value (line 1, column 1)
404 no route /other
```

## Going further

- Test `route` directly: build an `http.Request` in a `@test` and check the
  response, with no server (see `tasks/testing`).
- Serve `https://` with `http.serve_tls`, passing the certificate and key
  read from a `Dir`.
- Lower the limits for a public service with `http.serve_with` and
  `http.limits()` (see `library/http`).
- `examples/notes-api.la` keeps its data in an actor across requests.

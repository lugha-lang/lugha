---
title: JSON and TOML
summary: std.json and std.toml: parsing, values, writing, and encoding and decoding your own types.
modules: [json, json_value, toml, toml_value]
---

# JSON and TOML

## Overview

`import std.json;` parses and writes JSON, and turns your own types into
JSON and back. `import std.toml;` parses TOML configuration files. Both
parse into a `Value` enum you can `match`, with helpers (`get`, `as_text`,
…) for reaching inside, and both have limits so hostile input can't use up
memory.

## Reference

### std.json

| Item | What it does |
| --- | --- |
| `json.parse(text)` | `Ok(Value)`, or `Err(JsonError)` with the first mistake's line and column |
| `json.parse_with(text, limits)` | the same with other `Limits` |
| `json.stringify(v)` | compact text: `{"ok":true}`; `Err` (`NotFinite`) for NaN or an infinity |
| `json.pretty(v)` | text indented by two spaces |
| `Value` | `Null`, `Bool(bool)`, `Integer(i64)`, `Float(f64)`, `Text(string)`, `Array(Value[])`, `Object(Map<string, Value>)` |
| `json.get(v, key)`, `json.at(v, i)` | an object's member, an array's element: `Option<Value>` |
| `json.as_text(v)`, `json.as_i64(v)`, `json.as_f64(v)`, `json.as_bool(v)` | the value inside, if `v` is that kind |
| `json.as_array(v)`, `json.as_object(v)` | the elements or members, if `v` is that kind |
| `json.is_null(v)` | whether `v` is `null` |
| `Json` | the interface `encode` and `decode` take: derive it with `with json.Json` |
| `json.encode(x)` | `x` as a `Value`: structs as objects, enums by variant name, `None` as `null` |
| `json.decode(v)` | `Ok(x)` of the type the context asks for; `Err(DecodeError)` with a path such as `people[2].age` |
| `JsonError` | `kind`, `detail`, `line`, `column`, and `e.message()` |
| `ErrorKind` | `Syntax`, `Duplicate`, `Overflow`, `BadValue`, `Limit`, `NotFinite` |
| `DecodeError` | `path`, `detail`, and `e.message()` |
| `Limits`, `json.limits()` | `bytes` (1 MiB), `depth` (100), `values` (1 000 000) |

### std.toml

| Item | What it does |
| --- | --- |
| `toml.parse(text)` | `Ok(Map<string, Value>)`, the root table, or `Err(TomlError)` |
| `toml.parse_with(text, limits)` | the same with other `Limits` |
| `toml.get(table, "app.name")` | the value at a dotted path: `Option<Value>` |
| `toml.as_text`, `as_i64`, `as_f64`, `as_bool`, `as_array`, `as_table` | the value inside, if it is that kind |
| `toml.as_date_time(v)` | any date-time kind as `(Option<Date>, Option<Time>, Option<offset minutes>)` |
| `Value` | `Text`, `Integer`, `Float`, `Boolean`, `OffsetDateTime`, `LocalDateTime`, `LocalDate`, `LocalTime`, `Array`, `Table` |
| `Date` | `year`, `month`, `day` |
| `Time` | `hour`, `minute`, `second`, `nanosecond` |
| `TomlError` | `kind`, `detail`, `line`, `column`, and `e.message()` |
| `ErrorKind` | `Syntax`, `Duplicate`, `Immutable`, … |
| `Limits`, `toml.limits()` | `bytes`, `depth`, `values` |

## Examples

```lugha
import std.json;

struct Person { name: string, age: i64, email: Option<string>, tags: string[] } with json.Json
enum Shape { Dot, Circle(f64) } with json.Json

fun main(console: Console) {
    let text = "{{\"name\": \"Ann\", \"age\": 31, \"email\": null, \"tags\": [\"a\", \"b\"]}}";
    let v = json.parse(text).unwrap();
    console.println(json.get(v, "name").and_then(json.as_text).unwrap());
    let p: Person = json.decode(v).unwrap();
    console.println("{p.name} {p.age} {p.email.is_none()} {p.tags.len}");
    console.println(json.stringify(json.encode([Shape.Dot, Shape.Circle(1.5)])).unwrap());
    console.println(json.pretty(json.encode(Person { name: "Bo", age: 4, email: Some("b@x"), tags: [] })).unwrap());
    match json.parse("{{\"a\": 1,}}") {
        .Ok(_) => console.println("?"),
        .Err(e) => console.println(e.message()),
    }
    let wrong: Result<Person, json.DecodeError> = json.decode(json.parse("{{\"name\": 1}}").unwrap());
    match wrong {
        .Ok(_) => console.println("?"),
        .Err(e) => console.println(e.message()),
    }
}
```
```output
Ann
Ann 31 true 2
["Dot",{"Circle":1.5}]
{
  "name": "Bo",
  "age": 4,
  "email": "b@x",
  "tags": []
}
expected a key (line 1, column 9)
name: expected a string, found an integer
```

```lugha
import std.toml;

fun main(console: Console) {
    let text = "title = \"Shop\"\n\n[server]\nport = 8080\nhosts = [\"a\", \"b\"]\nstarted = 2026-11-20\n";
    let doc = toml.parse(text).unwrap();
    console.println(toml.get(doc, "title").and_then(toml.as_text).unwrap());
    console.println(toml.get(doc, "server.port").and_then(toml.as_i64).unwrap());
    let hosts = toml.get(doc, "server.hosts").and_then(toml.as_array).unwrap();
    console.println(hosts.len);
    let (date, time, offset) = toml.as_date_time(toml.get(doc, "server.started").unwrap()).unwrap();
    let d = date.unwrap();
    console.println("{d.year}-{d.month}-{d.day} {time.is_none()} {offset.is_none()}");
    match toml.parse("a = 1\na = 2\n") {
        .Ok(_) => console.println("?"),
        .Err(e) => console.println(e.message()),
    }
}
```
```output
Shop
8080
2
2026-11-20 true true
a key defined twice: a (line 2, column 1)
```

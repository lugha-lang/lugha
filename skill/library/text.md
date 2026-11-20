---
title: Text
summary: string and char methods, interpolation and formats, parsing numbers, bytes and UTF-8.
---

# Text

## Overview

`string` is immutable UTF-8 text and `char` one Unicode character. Both are
built in, with no import. Strings are compared, joined with `+` and built
with interpolation. Every position in a string is a **byte** offset, so a
character outside ASCII takes several positions.

## Reference

### string

| Item | What it gives |
| --- | --- |
| `s.len` | the length in bytes, an `i64` |
| `s.char_count()` | the number of characters |
| `s.chars()` | an iterator over the characters (`char`); invalid bytes give `'\u{FFFD}'` |
| `s.bytes()` | an iterator over the bytes (`u8`); `for b of s` does the same |
| `s.to_bytes()` | the bytes, as a new `u8[]` |
| `s.lines()` | a `string[]` split at `\n`, one `\r` dropped from each line, no empty last line |
| `s.split(sep)` | a `string[]` of the parts between each `sep`, empty parts kept; an empty `sep` panics |
| `s.trim()` | `s` without leading and trailing spaces, tabs and line breaks |
| `s.contains(sub)`, `s.starts_with(p)`, `s.ends_with(p)` | a `bool`; true for an empty argument |
| `s.slice(from, to)` | the bytes `from` up to `to`; panics unless `0 <= from <= to <= s.len` |
| `a + b` | the two strings joined |
| `==`, `<`, `>` … | comparison, byte by byte |
| `names.join(sep)` | a `string[]`'s items with `sep` between them |

### char

| Item | What it gives |
| --- | --- |
| `c.is_digit()` | `'0'` to `'9'` |
| `c.is_alphabetic()` | a letter (ASCII and Latin-1) |
| `c.is_upper()`, `c.is_lower()` | an upper- or lower-case letter |
| `c.is_whitespace()` | a space, tab, line break or other Unicode space |
| `c.to_upper()`, `c.to_lower()` | the other case, or `c` unchanged |
| `c as u32` | the code point; `n.try_into<char>()` goes back, `None` for a non-character |

### Building and reading text

| Item | What it gives |
| --- | --- |
| `"… {expr} …"` | interpolation: the value of any printable expression; `{{` and `}}` are braces |
| `"{x:spec}"` | a format, `[[fill]align][#][0][width][.precision][kind]`: `.2` (two decimals), `5` (width), `<5` `>5` `^5` (align), `*^7` (fill), `05` (zero-padded), `x` `X` `o` `b` (hex, octal, binary), `#x` (with `0x`) |
| `to_string(x)` | `x`'s text, as `"{x}"` |
| `x.debug()` | a readable form of any value: strings quoted, structs with their fields |
| `parse_i64(s)`, `parse_u64(s)` | `Some(n)` for an optional `-` (for `i64`) and digits, else `None` |
| `parse_f64(s)` | `Some(x)` for digits with an optional `.` part and exponent; `"inf"` and `".5"` give `None` |
| `string_from_utf8(bytes)` | `Ok(text)` when the `u8[]` is valid UTF-8, else `Err("invalid UTF-8 at byte N")` |
| `include_str("file.txt")` | the file's text, read when the program is compiled; the file must be UTF-8 and inside the package |

## Examples

```lugha
fun main(console: Console) {
    let line = "  name=Ann, age=31  ".trim();
    for part of line.split(", ") {
        let kv = part.split("=");
        console.println("{kv[0]:>5} -> {kv[1]}");
    }
    let word = "héllo";
    console.println("{word.len} bytes, {word.char_count()} chars, {word.slice(0, 1)}");
    let caps = word.chars().map(fun(c) = c.to_upper()).collect();
    console.println(caps.debug());
    console.println(["a", "b", "c"].join("/"));
    console.println("{3.14159:.2} {42:05} {255:#x} {7:b} {12:^6}|{"ok":*^6}");
    console.println("{parse_i64("-12").unwrap() * 2} {parse_f64("2.5e1").unwrap()}");
    console.println(string_from_utf8([104, 105]).unwrap());
    console.println(string_from_utf8([255]).is_err());
}
```
```output
 name -> Ann
  age -> 31
6 bytes, 5 chars, h
['H', 'É', 'L', 'L', 'O']
a/b/c
3.14 00042 0xff 111   12  |**ok**
-24 25.0
hi
true
```

```lugha
fun initials(name: string): string {
    let mut out = "";
    for word of name.split(" ") {
        for c of word.chars() {
            if c.is_alphabetic() {
                out = out + "{c.to_upper()}";
            }
            break;
        }
    }
    out
}

fun main(console: Console) {
    console.println(initials("ada lovelace"));
    console.println("a,b,,c".split(",").len);
    console.println("x\r\ny\n".lines().debug());
    console.println("{'7'.is_digit()} {' '.is_whitespace()} {'a' as u32}");
}
```
```output
AL
4
["x", "y"]
true true 97
```

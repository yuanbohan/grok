[![Build Status](https://github.com/yuanbohan/grok-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/yuanbohan/grok-rs/blob/main/.github/workflows/ci.yml)
[![Version](https://img.shields.io/crates/v/grok-rs?label=grok-rs)](https://crates.io/crates/grok-rs)
[![codecov](https://codecov.io/gh/yuanbohan/grok-rs/graph/badge.svg?token=1T8WSFV6BX)](https://codecov.io/gh/yuanbohan/grok-rs)

# grok_rs

the `grok_rs` is a rust port of Elastic Grok processor, inspired by [grok-go][grok-go] and [grok][grok]

## Usage

```toml
[dependencies]
grok-rs = "0.1"
```

## Example

### Only with default patterns

```rust
let grok = Grok::default();
let pattern = grok
    // USERNAME are defined in grok-patterns
    .compile("%{USERNAME}", false)
    .unwrap();
let result = pattern.parse("admin admin@example.com").unwrap();
println!("{:#?}", result);
```

the output is:

```text
{
    "USERNAME": String(
        "admin",
    ),
}
```

### With user-defined patterns

```rust
let mut grok = Grok::default();
grok.add_pattern("NAME", r"[A-z0-9._-]+");
let pattern = grok.compile("%{NAME}", false).unwrap();
let result = pattern.parse("admin").unwrap();
println!("{:#?}", result);
```

the output is:

```text
{
    "NAME": String(
        "admin",
    ),
}
```

### With `named_capture_only` is true

```rust
let grok = Grok::default();
let pattern = grok
    .compile("%{USERNAME} %{EMAILADDRESS:email}", true)
    .unwrap();
let result = pattern.parse("admin admin@example.com").unwrap();
println!("{:#?}", result);
```

the output is:

```text
{
    "email": String(
        "admin@example.com",
    ),
}
```

### With type

```rust
let mut grok = Grok::default();
grok.add_pattern("NUMBER", r"\d+");

let pattern = grok.compile("%{NUMBER:digit:int}", false).unwrap();
let result = pattern.parse("hello 123").unwrap();
println!("{:#?}", result);
```

the output is:

```text
{
    "digit": Int(
        123,
    ),
}
```

## Notice

`grok_rs` is based on [regex][regex] crate, so lacks several features that are not known how to implement efficiently. This includes, but is not limited to, look-around and backreferences. In exchange, all regex searches in this crate have worst case `O(m * n)` time complexity, where `m` is proportional to the size of the regex and `n` is proportional to the size of the string being searched.

## Elastic Grok compliance

This crate declares compatible with [elastic grok patterns v8.14.0][grok-patterns], which is tagged at 2024-06-05.

[grok-patterns]: https://github.com/elastic/elasticsearch/tree/v8.14.0/libs/grok/src/main/resources/patterns/ecs-v1
[grok-go]: https://github.com/elastic/go-grok
[grok]: https://github.com/daschl/grok
[regex]: https://docs.rs/regex/latest/regex

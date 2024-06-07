# grok

Rust port of Elastic Grok processor, inspired by [grok-go][grok-go] and [grok][grok]

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

## Elastic Grok compliance

This crate declares compatible with [elastic grok patterns v8.14.0][grok-patterns], which is tagged at 2024-06-05.

[grok-patterns]: https://github.com/elastic/elasticsearch/tree/v8.14.0/libs/grok/src/main/resources/patterns/ecs-v1
[grok-go]: https://github.com/elastic/go-grok
[grok]: https://github.com/daschl/grok

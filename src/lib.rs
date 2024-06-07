use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
};

use glob::glob;
use lazy_static::lazy_static;
use regex::Regex;

const MAX_RECURSION: i32 = 1024;

const NAME_INDEX: usize = 1;
const PATTERN_INDEX: usize = 2;
const ALIAS_INDEX: usize = 3;
const TYPE_INDEX: usize = 4;

const GROK_PATTERN: &str = r"(?x)
%\{
    (?<name>
        (?<pattern>[[:word:]]+)
        (?:
            :(?<alias>[[[:word:]]@.-]+)
            (?:
                :(?<type>int|float|bool(?:ean)?)
            )?
        )?
    )
\}";

fn load_patterns() -> HashMap<String, String> {
    let mut patterns = HashMap::new();

    for line in glob("src/patterns/*")
        .unwrap()
        .map(|e| File::open(e.unwrap()).unwrap())
        .flat_map(|f| BufReader::new(f).lines())
        .map(|line| line.unwrap())
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let (key, value) = line.split_at(line.find(' ').unwrap());
        patterns.insert(key.to_string(), value.trim().to_string());
    }

    patterns.insert("BOOL".into(), "true|false".into());

    patterns
}

lazy_static! {
    static ref GROK_REGEX: Regex = Regex::new(GROK_PATTERN).unwrap();
    static ref DEFAULT_PATTERNS: HashMap<String, String> = load_patterns();
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

#[derive(Debug)]
pub struct Pattern {
    regex: Regex,
    alias: HashMap<String, (String, Option<String>)>,
}

impl Pattern {
    fn new(regex: Regex, alias: HashMap<String, (String, Option<String>)>) -> Self {
        Self { regex, alias }
    }

    pub fn parse(&self, s: &str) -> Result<HashMap<String, Value>, String> {
        let mut map = HashMap::new();
        let names = self.regex.capture_names().flatten().collect::<Vec<_>>();

        let caps = match self.regex.captures(s) {
            Some(caps) => caps,
            None => return Ok(map),
        };

        for name in names {
            if let Some(m) = caps.name(name) {
                let value = m.as_str().to_string();
                match self.alias.get(name) {
                    Some((alias, type_)) => {
                        let value = match type_ {
                            Some(type_) => match type_.as_str() {
                                "int" => Value::Int(
                                    value.parse::<i64>().map_err(|e| format!("{e}: {value}"))?,
                                ),
                                "float" => Value::Float(
                                    value.parse::<f64>().map_err(|e| format!("{e}: {value}"))?,
                                ),
                                "bool" | "boolean" => Value::Bool(
                                    value.parse::<bool>().map_err(|e| format!("{e}: {value}"))?,
                                ),
                                _ => Value::String(value),
                            },
                            None => Value::String(value),
                        };
                        map.insert(alias.clone(), value);
                    }
                    None => {
                        map.insert(name.to_string(), Value::String(value));
                    }
                }
            }
        }

        Ok(map)
    }
}

#[derive(Default, Debug)]
pub struct Grok {
    patterns: HashMap<String, String>,
}

impl Grok {
    pub fn add_pattern<T: Into<String>>(&mut self, name: T, pattern: T) {
        self.patterns.insert(name.into(), pattern.into());
    }

    /// if named_capture_only is true, then pattern without alias won't be captured. e.g.
    /// if pattern is "%{USERNAME} %{EMAILADDRESS:email}" and named_capture_only is true,
    /// then only email will be captured.
    pub fn compile(&self, s: &str, named_capture_only: bool) -> Result<Pattern, String> {
        let mut alias_map = HashMap::new();
        let mut haystack = s.to_string();
        let mut index = 0;
        let mut iter_left = MAX_RECURSION;

        while let Some(caps) = GROK_REGEX.captures(haystack.clone().as_str()) {
            if iter_left <= 0 {
                return Err(format!("max recursion {MAX_RECURSION} reached"));
            }
            iter_left -= 1;

            let name = caps.get(NAME_INDEX).ok_or("name not found")?.as_str();
            let pattern = caps.get(PATTERN_INDEX).ok_or("pattern not found")?.as_str();

            let pattern_regex = self
                .patterns
                .get(pattern)
                .or(DEFAULT_PATTERNS.get(pattern))
                .ok_or(format!("pattern: {pattern}  not found"))?;

            let to_replace = format!("%{{{name}}}");

            while haystack.matches(&to_replace).count() > 0 {
                let replacement = match caps.get(ALIAS_INDEX) {
                    None if named_capture_only => {
                        format!("(?:{pattern_regex})")
                    }
                    _ => {
                        let new_name = format!("name{index}");
                        let origin_alias =
                            caps.get(ALIAS_INDEX).map(|m| m.as_str()).unwrap_or(pattern);
                        let type_ = caps.get(TYPE_INDEX).map(|m| m.as_str().to_string());
                        alias_map.insert(new_name.clone(), (origin_alias.to_string(), type_));
                        format!("(?<{new_name}>{pattern_regex})")
                    }
                };

                haystack = haystack.replacen(&to_replace, &replacement, 1);
                index += 1;
            }
        }

        let re = Regex::new(haystack.as_str()).map_err(|e| e.to_string())?;
        Ok(Pattern::new(re, alias_map))
    }
}

impl<T: Into<String>> FromIterator<(T, T)> for Grok {
    fn from_iter<I: IntoIterator<Item = (T, T)>>(iter: I) -> Self {
        let mut grok = Grok::default();
        for (k, v) in iter {
            grok.add_pattern(k, v);
        }
        grok
    }
}

impl<S: Into<String>, const N: usize> From<[(S, S); N]> for Grok {
    fn from(arr: [(S, S); N]) -> Self {
        Self::from_iter(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Case<'a> {
        patterns: Vec<(&'a str, &'a str)>,
        pattern: &'a str,
        input: &'a str,
        expected: HashMap<String, Value>,
        named_capture_only: bool,
    }

    fn assert(c: Case<'_>) {
        let grok = Grok::from_iter(c.patterns);
        let pattern = grok.compile(c.pattern, c.named_capture_only).unwrap();
        assert_eq!(c.expected, pattern.parse(c.input).unwrap());
    }

    fn asserts(cases: Vec<Case<'_>>) {
        for c in cases {
            assert(c);
        }
    }

    #[test]
    fn test_simple_add_pattern() {
        let mut grok = Grok::default();
        grok.add_pattern("NAME", r"[A-z0-9._-]+");
        let pattern = grok.compile("%{NAME}", false).unwrap();
        let expected: HashMap<String, Value> = [("NAME", "admin")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();

        assert_eq!(expected, pattern.parse("admin").unwrap());
        assert_eq!(expected, pattern.parse("admin user").unwrap());
    }

    #[test]
    fn test_named_capture_only() {
        let grok = Grok::default();
        let pattern = grok
            // USERNAME and EMAILADDRESS are defined in grok-patterns
            .compile("%{USERNAME} %{EMAILADDRESS:email}", true)
            .unwrap();

        let expected = [("email", "admin@example.com")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect::<HashMap<String, Value>>();

        assert_eq!(expected, pattern.parse("admin admin@example.com").unwrap());
    }

    #[test]
    fn test_from() {
        let expected = [("NAME", "admin")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect::<HashMap<String, Value>>();

        {
            let grok = Grok::from_iter([("NAME", r"[A-z0-9._-]+")]);
            let pattern = grok.compile("%{NAME}", false).unwrap();
            assert_eq!(expected, pattern.parse("admin").unwrap());
        }
        {
            let grok = Grok::from([("NAME", r"[A-z0-9._-]+")]);
            let pattern = grok.compile("%{NAME}", false).unwrap();
            assert_eq!(expected, pattern.parse("admin").unwrap());
        }
    }

    #[test]
    fn test_composite_or_pattern() {
        let mut grok = Grok::default();
        grok.add_pattern("MAC", r"(?:%{CISCOMAC}|%{WINDOWSMAC}|%{COMMONMAC})");
        grok.add_pattern("CISCOMAC", r"(?:(?:[A-Fa-f0-9]{4}\.){2}[A-Fa-f0-9]{4})");
        grok.add_pattern("WINDOWSMAC", r"(?:(?:[A-Fa-f0-9]{2}-){5}[A-Fa-f0-9]{2})");
        grok.add_pattern("COMMONMAC", r"(?:(?:[A-Fa-f0-9]{2}:){5}[A-Fa-f0-9]{2})");

        let pattern = grok.compile("%{MAC}", false).unwrap();
        let expected = [
            ("MAC", "5E:FF:56:A2:AF:15"),
            ("COMMONMAC", "5E:FF:56:A2:AF:15"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect::<HashMap<String, Value>>();

        assert_eq!(expected, pattern.parse("5E:FF:56:A2:AF:15").unwrap());
        assert_eq!(
            expected,
            pattern.parse("127.0.0.1 5E:FF:56:A2:AF:15").unwrap()
        );
    }

    #[test]
    fn test_multiple_patterns() {
        let mut grok = Grok::default();
        grok.add_pattern("YEAR", r"(\d\d){1,2}");
        grok.add_pattern("MONTH", r"\b(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\b");
        grok.add_pattern("DAY", r"(?:Mon(?:day)?|Tue(?:sday)?|Wed(?:nesday)?|Thu(?:rsday)?|Fri(?:day)?|Sat(?:urday)?|Sun(?:day)?)");
        let pattern = grok.compile("%{DAY} %{MONTH} %{YEAR}", false).unwrap();

        let expected = [("DAY", "Monday"), ("MONTH", "March"), ("YEAR", "2012")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect::<HashMap<String, Value>>();
        assert_eq!(expected, pattern.parse("Monday March 2012").unwrap());
    }

    #[test]
    fn test_adhoc_pattern() {
        let grok = Grok::default();
        let pattern = grok.compile(r"\[(?<threadname>[^\]]+)\]", false).unwrap();
        let expected = [("threadname", "thread1")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect::<HashMap<String, Value>>();
        assert_eq!(expected, pattern.parse("[thread1]").unwrap());
    }

    #[test]
    fn test_type() {
        let mut grok = Grok::default();
        grok.add_pattern("NUMBER", r"\d+");

        // int
        {
            let pattern = grok.compile("%{NUMBER:digit:int}", false).unwrap();
            let expected = [("digit", Value::Int(123))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<HashMap<String, Value>>();
            assert_eq!(expected, pattern.parse("hello 123").unwrap());
        }

        // float
        {
            let pattern = grok.compile("%{NUMBER:digit:float}", false).unwrap();
            let expected = [("digit", Value::Float(123.0))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<HashMap<String, Value>>();
            assert_eq!(expected, pattern.parse("hello 123.0").unwrap());
        }

        // wrong type
        {
            let pattern = grok.compile("%{NUMBER:digit:wrong}", false);
            assert!(pattern.is_err());
        }

        {
            // wrong value
            let pattern = grok.compile("%{USERNAME:digit:float}", false).unwrap();
            assert_eq!(
                Err("invalid float literal: grok".to_string()),
                pattern.parse("grok")
            );
        }
    }

    #[test]
    fn test_more_patterns() {
        let cases: Vec<Case> = [(
            vec![
                (
                    "NGINX_HOST",
                    r#"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"#,
                ),
                ("IP", r#"(?:\[%{IPV6}\]|%{IPV6}|%{IPV4})"#),
                ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                ("NUMBER", r#"\d+"#),
                (
                    "IPV6",
                    r#"((([0-9A-Fa-f]{1,4}:){7}([0-9A-Fa-f]{1,4}|:))|(([0-9A-Fa-f]{1,4}:){6}(:[0-9A-Fa-f]{1,4}|((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){5}(((:[0-9A-Fa-f]{1,4}){1,2})|:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){4}(((:[0-9A-Fa-f]{1,4}){1,3})|((:[0-9A-Fa-f]{1,4})?:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){3}(((:[0-9A-Fa-f]{1,4}){1,4})|((:[0-9A-Fa-f]{1,4}){0,2}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){2}(((:[0-9A-Fa-f]{1,4}){1,5})|((:[0-9A-Fa-f]{1,4}){0,3}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){1}(((:[0-9A-Fa-f]{1,4}){1,6})|((:[0-9A-Fa-f]{1,4}){0,4}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(:(((:[0-9A-Fa-f]{1,4}){1,7})|((:[0-9A-Fa-f]{1,4}){0,5}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:)))(%.+)?"#,
                ),
                (
                    "IPV4",
                    r#"\b(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\b"#,
                ),
            ],
            "%{NGINX_HOST}",
            "127.0.0.1:1234",
            vec![
                ("destination.ip", Value::String("127.0.0.1".to_string())),
                ("destination.port", Value::String("1234".to_string())),
            ],
            true,
        ),
        (
            vec![
                (
                    "NGINX_HOST",
                    r#"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"#,
                ),
                ("IP", r#"(?:\[%{IPV6}\]|%{IPV6}|%{IPV4})"#),
                ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                ("NUMBER", r#"\d+"#),
                (
                    "IPV6",
                    r#"((([0-9A-Fa-f]{1,4}:){7}([0-9A-Fa-f]{1,4}|:))|(([0-9A-Fa-f]{1,4}:){6}(:[0-9A-Fa-f]{1,4}|((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){5}(((:[0-9A-Fa-f]{1,4}){1,2})|:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){4}(((:[0-9A-Fa-f]{1,4}){1,3})|((:[0-9A-Fa-f]{1,4})?:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){3}(((:[0-9A-Fa-f]{1,4}){1,4})|((:[0-9A-Fa-f]{1,4}){0,2}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){2}(((:[0-9A-Fa-f]{1,4}){1,5})|((:[0-9A-Fa-f]{1,4}){0,3}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){1}(((:[0-9A-Fa-f]{1,4}){1,6})|((:[0-9A-Fa-f]{1,4}){0,4}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(:(((:[0-9A-Fa-f]{1,4}){1,7})|((:[0-9A-Fa-f]{1,4}){0,5}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:)))(%.+)?"#,
                ),
                (
                    "IPV4",
                    r#"\b(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\.(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])\b"#,
                ),
            ],
            "%{NGINX_HOST}",
            "127.0.0.1:1234",
            vec![
                ("destination.ip", Value::String("127.0.0.1".to_string())),
                ("destination.port", Value::String("1234".to_string())),
                ("NGINX_HOST", Value::String("127.0.0.1:1234".to_string())),
                ("IPV4", Value::String("127.0.0.1".to_string())),
            ],
            false,
        )
        ].into_iter().map(|(patterns, pattern, input, expected, named_capture_only)| Case {
            patterns: patterns.into_iter().collect(),
            pattern,
            input,
            expected: expected.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            named_capture_only,
        }).collect();

        asserts(cases);
    }

    #[test]
    fn test_default_patterns() {
        let cases: Vec<Case> = [
            (
                vec![
                    ("NGINX_HOST",         r"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"),
                    ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                ],
                "%{NGINX_HOST}",
                "127.0.0.1:1234",
                vec![
                    ("destination.ip", Value::String("127.0.0.1".to_string())),
                    ("destination.port", Value::String("1234".to_string())),
                ],
                true,
            ),
            (
                vec![
                    ("NGINX_HOST",         r"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"),
                    ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                ],
                "%{NGINX_HOST}",
                "127.0.0.1:1234",
                vec![
                    ("destination.ip", Value::String("127.0.0.1".to_string())),
                    ("destination.port", Value::String("1234".to_string())),
                    ("BASE10NUM", Value::String("1234".to_string())),
                    ("NGINX_HOST", Value::String("127.0.0.1:1234".to_string())),
                    ("IPV4", Value::String("127.0.0.1".to_string())),
                ],
                false,
            ),
        ]
        .into_iter()
        .map(
            |(patterns, pattern, input, expected, named_capture_only)| Case {
                patterns: patterns.into_iter().collect(),
                pattern,
                input,
                expected: expected
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                named_capture_only,
            },
        )
        .collect();

        asserts(cases);
    }

    #[test]
    fn test_default_patterns_with_type() {
        let cases: Vec<Case> = [
            (
                vec![
                    ("NGINX_HOST",         r"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"),
                    ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                ],
                "%{NGINX_HOST}",
                "127.0.0.1:1234",
                vec![
                    ("destination.ip", Value::String("127.0.0.1".to_string())),
                    ("destination.port", Value::String("1234".to_string())),
                    ("BASE10NUM", Value::String("1234".to_string())),
                    ("NGINX_HOST", Value::String("127.0.0.1:1234".to_string())),
                    ("IPV4", Value::String("127.0.0.1".to_string())),
                ],
                false,
            ),
            (
                vec![
                    ("NGINX_HOST",         r#"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port:int})?"#),
                    ("NGINX_NOTSEPARATOR", r#"[^\t ,:]+"#),
                    ("BOOL", r#"true|false"#),
                ],
                "%{NGINX_HOST} %{BOOL:destination.boolean:boolean}",
                "127.0.0.1:1234 true",
                vec![
                    ("destination.ip", Value::String("127.0.0.1".to_string())),
                    ("destination.port", Value::Int(1234)),
                    ("destination.boolean", Value::Bool(true)),
                ],
                true,
            ),
        ]
        .into_iter()
        .map(
            |(patterns, pattern, input, expected, named_capture_only)| Case {
                patterns: patterns.into_iter().collect(),
                pattern,
                input,
                expected: expected
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                named_capture_only,
            },
        )
        .collect();

        asserts(cases);
    }

    #[test]
    fn test_more_default_patterns() {
        let cases = [
            ("WORD", vec!["hello", "world123", "test_data"]),
            ("NOTSPACE", vec!["example", "text-with-dashes", "12345"]),
            ("SPACE", vec![" ", "\t", "  "]),
            // types
            ("INT", vec!["123", "-456", "+789"]),
            ("NUMBER", vec!["123", "456.789", "-0.123"]),
            ("BOOL", vec!["true", "false", "true"]),
            ("BASE10NUM", vec!["123", "-123.456", "0.789"]),
            ("BASE16NUM", vec!["1a2b", "0x1A2B", "-0x1a2b3c"]),
            ("BASE16FLOAT", vec!["0x1.a2b3", "-0x1A2B3C.D", "0x123.abc"]),
            ("POSINT", vec!["123", "456", "789"]),
            ("NONNEGINT", vec!["0", "123", "456"]),
            (
                "GREEDYDATA",
                vec!["anything goes", "literally anything", "123 #@!"],
            ),
            (
                "QUOTEDSTRING",
                vec!["\"This is a quote\"", "'single quoted'"],
            ),
            (
                "UUID",
                vec![
                    "123e4567-e89b-12d3-a456-426614174000",
                    "123e4567-e89b-12d3-a456-426614174001",
                    "123e4567-e89b-12d3-a456-426614174002",
                ],
            ),
            (
                "URN",
                vec![
                    "urn:isbn:0451450523",
                    "urn:ietf:rfc:2648",
                    "urn:mpeg:mpeg7:schema:2001",
                ],
            ),
            // network
            (
                "IP",
                vec![
                    "192.168.1.1",
                    "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
                    "172.16.254.1",
                ],
            ),
            (
                "IPV6",
                vec![
                    "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
                    "::1",
                    "fe80::1ff:fe23:4567:890a",
                ],
            ),
            ("IPV4", vec!["192.168.1.1", "10.0.0.1", "172.16.254.1"]),
            (
                "IPORHOST",
                vec!["example.com", "192.168.1.1", "fe80::1ff:fe23:4567:890a"],
            ),
            (
                "HOSTNAME",
                vec!["example.com", "sub.domain.co.uk", "localhost"],
            ),
            ("EMAILLOCALPART", vec!["john.doe", "alice123", "bob-smith"]),
            (
                "EMAILADDRESS",
                vec![
                    "john.doe@example.com",
                    "alice123@domain.co.uk",
                    "bob-smith@localhost",
                ],
            ),
            ("USERNAME", vec!["user1", "john.doe", "alice_123"]),
            ("USER", vec!["user1", "john.doe", "alice_123"]),
            (
                "MAC",
                vec!["00:1A:2B:3C:4D:5E", "001A.2B3C.4D5E", "00-1A-2B-3C-4D-5E"],
            ),
            (
                "CISCOMAC",
                vec!["001A.2B3C.4D5E", "001B.2C3D.4E5F", "001C.2D3E.4F5A"],
            ),
            (
                "WINDOWSMAC",
                vec![
                    "00-1A-2B-3C-4D-5E",
                    "00-1B-2C-3D-4E-5F",
                    "00-1C-2D-3E-4F-5A",
                ],
            ),
            (
                "COMMONMAC",
                vec![
                    "00:1A:2B:3C:4D:5E",
                    "00:1B:2C:3D:4E:5F",
                    "00:1C:2D:3E:4F:5A",
                ],
            ),
            ("HOSTPORT", vec!["example.com:80", "192.168.1.1:8080"]),
            // paths
            (
                "UNIXPATH",
                vec!["/home/user", "/var/log/syslog", "/tmp/abc_123"],
            ),
            ("TTY", vec!["/dev/pts/1", "/dev/tty0", "/dev/ttyS0"]),
            (
                "WINPATH",
                vec![
                    "C:\\Program Files\\App",
                    "D:\\Work\\project\\file.txt",
                    "E:\\New Folder\\test",
                ],
            ),
            ("URIPROTO", vec!["http", "https", "ftp"]),
            ("URIHOST", vec!["example.com", "192.168.1.1:8080"]),
            (
                "URIPATH",
                vec!["/path/to/resource", "/another/path", "/root"],
            ),
            (
                "URIQUERY",
                vec!["key=value", "name=John&Doe", "search=query&active=true"],
            ),
            (
                "URIPARAM",
                vec!["?key=value", "?name=John&Doe", "?search=query&active=true"],
            ),
            (
                "URIPATHPARAM",
                vec![
                    "/path?query=1",
                    "/resource?name=John",
                    "/folder/path?valid=true",
                ],
            ),
            (
                "URI",
                vec![
                    "http://user:password@example.com:80/path?query=string",
                    "https://example.com",
                    "ftp://192.168.1.1/upload",
                ],
            ),
            (
                "PATH",
                vec![
                    "/home/user/documents",
                    "C:\\Windows\\system32",
                    "/var/log/syslog",
                ],
            ),
            // dates
            (
                "MONTH",
                vec![
                    "January",
                    "Feb",
                    "March",
                    "Apr",
                    "May",
                    "Jun",
                    "Jul",
                    "August",
                    "September",
                    "October",
                    "Nov",
                    "December",
                ],
            ),
            // Months: January, Feb, 3, 03, 12, December "MONTH": `\b(?:[Jj]an(?:uary|uar)?|[Ff]eb(?:ruary|ruar)?|[Mm](?:a|ä)?r(?:ch|z)?|[Aa]pr(?:il)?|[Mm]a(?:y|i)?|[Jj]un(?:e|i)?|[Jj]ul(?:y|i)?|[Aa]ug(?:ust)?|[Ss]ep(?:tember)?|[Oo](?:c|k)?t(?:ober)?|[Nn]ov(?:ember)?|[Dd]e(?:c|z)(?:ember)?)\b`,
            (
                "MONTHNUM2",
                vec![
                    "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
                ],
            ),
            // Days Monday, Tue, Thu, etc
            (
                "DAY",
                vec![
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                    "Sunday",
                ],
            ),
            // Years?
            ("YEAR", vec!["1999", "2000", "2021"]),
            ("HOUR", vec!["00", "12", "23"]),
            ("MINUTE", vec!["00", "30", "59"]),
            // '60' is a leap second in most time standards and thus is valid.
            ("SECOND", vec!["00", "30", "60"]),
            ("TIME", vec!["14:30", "23:59:59", "12:00:00", "12:00:60"]),
            // datestamp is YYYY/MM/DD-HH:MM:SS.UUUU (or something like it)
            ("DATE_US", vec!["04/21/2022", "12-25-2020", "07/04/1999"]),
            ("DATE_EU", vec!["21.04.2022", "25/12/2020", "04-07-1999"]),
            ("ISO8601_TIMEZONE", vec!["Z", "+02:00", "-05:00"]),
            ("ISO8601_SECOND", vec!["59", "30", "60.123"]),
            (
                "TIMESTAMP_ISO8601",
                vec![
                    "2022-04-21T14:30:00Z",
                    "2020-12-25T23:59:59+02:00",
                    "1999-07-04T12:00:00-05:00",
                ],
            ),
            ("DATE", vec!["04/21/2022", "21.04.2022", "12-25-2020"]),
            (
                "DATESTAMP",
                vec!["04/21/2022 14:30", "21.04.2022 23:59", "12-25-2020 12:00"],
            ),
            ("TZ", vec!["EST", "CET", "PDT"]),
            ("DATESTAMP_RFC822", vec!["Wed Jan 12 2024 14:33 EST"]),
            (
                "DATESTAMP_RFC2822",
                vec![
                    "Tue, 12 Jan 2022 14:30 +0200",
                    "Fri, 25 Dec 2020 23:59 -0500",
                    "Sun, 04 Jul 1999 12:00 Z",
                ],
            ),
            (
                "DATESTAMP_OTHER",
                vec![
                    "Tue Jan 12 14:30 EST 2022",
                    "Fri Dec 25 23:59 CET 2020",
                    "Sun Jul 04 12:00 PDT 1999",
                ],
            ),
            (
                "DATESTAMP_EVENTLOG",
                vec!["20220421143000", "20201225235959", "19990704120000"],
            ),
            // Syslog Dates: Month Day HH:MM:SS	"MONTH":         `\b(?:Jan(?:uary|uar)?|Feb(?:ruary|ruar)?|Mar(?:ch|z)?|Apr(?:il)?|May|i|Jun(?:e|i)?|Jul(?:y|i)?|Aug(?:ust)?|Sep(?:tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\b`,
            (
                "SYSLOGTIMESTAMP",
                vec!["Jan  1 00:00:00", "Mar 15 12:34:56", "Dec 31 23:59:59"],
            ),
            ("PROG", vec!["sshd", "kernel", "cron"]),
            ("SYSLOGPROG", vec!["sshd[1234]", "kernel", "cron[5678]"]),
            (
                "SYSLOGHOST",
                vec!["example.com", "192.168.1.1", "localhost"],
            ),
            ("SYSLOGFACILITY", vec!["<1.2>", "<12345.13456>"]),
            ("HTTPDATE", vec!["25/Dec/2024:14:33 4"]),
        ];

        for (pattern, values) in cases {
            let grok = Grok::default();
            let p = grok
                .compile(&format!("%{{{pattern}:result}}"), true)
                .unwrap();

            for value in values {
                let m = p.parse(value).unwrap();
                let result = m.get("result").unwrap();
                assert_eq!(
                    &Value::String(value.to_string()),
                    result,
                    "pattern: {}, value: {}",
                    pattern,
                    value
                );
            }
        }
    }
}

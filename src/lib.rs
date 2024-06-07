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
                :(?<type>int|float)
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

        for (name, (alias, type_)) in self.alias.iter() {
            if let Some(m) = caps.name(name) {
                let value = m.as_str().to_string();
                let value = match type_ {
                    Some(type_) => match type_.as_str() {
                        "int" => Value::Int(value.parse::<i64>().map_err(|e| e.to_string())?),
                        "float" => Value::Float(value.parse::<f64>().map_err(|e| e.to_string())?),
                        _ => Value::String(value),
                    },
                    None => Value::String(value),
                };
                map.insert(alias.clone(), value);
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

            println!("{pattern}: {pattern_regex}");

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

        println!("haystack: {:?}", haystack);
        let re = Regex::new(haystack.as_str()).map_err(|e| e.to_string())?;
        println!("re: {:?}", re);
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

    fn assert<'a>(c: Case<'a>) {
        let grok = Grok::from_iter(c.patterns.into_iter());
        let pattern = grok.compile(c.pattern, c.named_capture_only).unwrap();
        assert_eq!(c.expected, pattern.parse(c.input).unwrap());
    }

    fn asserts<'a>(cases: Vec<Case<'a>>) {
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
            let grok = Grok::from_iter([("NAME", r"[A-z0-9._-]+")].into_iter());
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
                ("NGINX_NOTSEPARATOR", r#""[^\t ,:]+""#),
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
                ("NGINX_NOTSEPARATOR", r#""[^\t ,:]+""#),
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
            patterns: patterns.into_iter().map(|(k, v)| (k, v)).collect(),
            pattern,
            input,
            expected: expected.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            named_capture_only,
        }).collect();

        asserts(cases);
    }

    #[test]
    fn test_default_patterns() {
        let cases: Vec<Case> = [(
            vec![
                ("NGINX_HOST",         r"(?:%{IP:destination.ip}|%{NGINX_NOTSEPARATOR:destination.domain})(:%{NUMBER:destination.port})?"),
                ("NGINX_NOTSEPARATOR", r#""[^\t ,:]+""#),
            ],
            "%{NGINX_HOST}",
            "127.0.0.1:1234",
            vec![
                ("destination.ip", Value::String("127.0.0.1".to_string())),
                ("destination.port", Value::String("1234".to_string())),
            ],
            true,
        )]
        .into_iter()
        .map(
            |(patterns, pattern, input, expected, named_capture_only)| Case {
                patterns: patterns.into_iter().map(|(k, v)| (k, v)).collect(),
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
}

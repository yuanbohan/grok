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
        .map(|e| e.unwrap())
        .map(|path| File::open(path).unwrap())
        .flat_map(|f| BufReader::new(f).lines())
        .map(|line| line.unwrap())
        .filter(|line| !line.starts_with('#'))
        .filter(|line| !line.is_empty())
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

#[derive(Debug)]
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

        let caps = match self.regex.captures(s) {
            Some(caps) => caps,
            None => return Ok(map),
        };

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
    pub fn add_pattern(&mut self, name: impl Into<String>, pattern: impl Into<String>) {
        self.patterns.insert(name.into(), pattern.into());
    }

    pub fn compile(&self, s: &str, with_alias_only: bool) -> Result<Pattern, String> {
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
                .ok_or(format!("pattern: {pattern}  not found"))?;

            let to_match = format!("%{{{name}}}");

            while haystack.matches(&to_match).count() > 0 {
                let replacement = match caps.get(ALIAS_INDEX) {
                    None if with_alias_only => {
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

                haystack = haystack.replacen(&to_match, &replacement, 1);
                index += 1;
            }
        }

        let re = Regex::new(haystack.as_str()).map_err(|e| e.to_string())?;
        Ok(Pattern::new(re, alias_map))
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use crate::{load_patterns, Grok, GROK_PATTERN};
}

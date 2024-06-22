//! Generating of JSON documents.
//!
//! This module provides a simple way to generate JSON documents, without the
//! need for big third-party dependencies (e.g. serde), and only covering what
//! we need for the compiler.
//!
//! The implementation here is based on `std.json` from Inko's standard library.
use std::string::ToString;

const DQUOTE: i64 = 0x22;
const BSLASH: i64 = 0x5C;
const LOWER_B: i64 = 0x62;
const LOWER_N: i64 = 0x6e;
const LOWER_F: i64 = 0x66;
const LOWER_R: i64 = 0x72;
const LOWER_T: i64 = 0x74;

const ESCAPE_TABLE: [i64; 96] = [
    -1, -1, -1, -1, -1, -1, -1, -1, LOWER_B, LOWER_T, LOWER_N, -1, LOWER_F,
    LOWER_R, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, DQUOTE, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, BSLASH, -1, -1, -1,
];

fn escaped(value: &str) -> String {
    let mut buf = Vec::new();

    for byte in value.bytes() {
        match ESCAPE_TABLE.get(byte as usize) {
            Some(-1) | None => buf.push(byte),
            Some(&byte) => {
                buf.push(BSLASH as u8);
                buf.push(byte as u8);
            }
        }
    }

    String::from_utf8_lossy(&buf).into_owned()
}

pub(crate) struct Object {
    pairs: Vec<(String, Json)>,
}

impl Object {
    pub(crate) fn new() -> Object {
        Object { pairs: Vec::new() }
    }

    pub(crate) fn add(&mut self, key: &str, value: Json) {
        self.pairs.push((key.to_string(), value));
    }
}

pub(crate) enum Json {
    Int(i64),
    String(String),
    Array(Vec<Json>),
    Object(Object),
    Bool(bool),
}

impl ToString for Json {
    fn to_string(&self) -> String {
        Generator::new().generate(self)
    }
}

pub struct Generator {
    depth: usize,
    buffer: String,
}

impl Generator {
    fn new() -> Generator {
        Generator { depth: 0, buffer: String::new() }
    }

    fn generate(mut self, value: &Json) -> String {
        self.generate_value(value);
        self.buffer
    }

    fn generate_value(&mut self, value: &Json) {
        match value {
            Json::Int(val) => self.buffer.push_str(&val.to_string()),
            Json::String(val) => {
                self.buffer.push('"');
                self.buffer.push_str(&escaped(val));
                self.buffer.push('"');
            }
            Json::Array(vals) => {
                self.buffer.push('[');

                if !vals.is_empty() {
                    self.enter(|this| {
                        for (idx, val) in vals.iter().enumerate() {
                            if idx > 0 {
                                this.buffer.push_str(",\n");
                            }

                            this.indent();
                            this.generate_value(val);
                        }
                    });

                    self.indent();
                }

                self.buffer.push(']');
            }
            Json::Object(vals) => {
                self.buffer.push('{');

                if !vals.pairs.is_empty() {
                    self.enter(|this| {
                        for (idx, (k, v)) in vals.pairs.iter().enumerate() {
                            if idx > 0 {
                                this.buffer.push_str(",\n");
                            }

                            this.indent();
                            this.buffer.push('"');
                            this.buffer.push_str(&escaped(k));
                            this.buffer.push_str("\": ");
                            this.generate_value(v);
                        }
                    });

                    self.indent();
                }

                self.buffer.push('}');
            }
            Json::Bool(v) => self.buffer.push_str(&v.to_string()),
        }
    }

    fn enter<F: FnMut(&mut Generator)>(&mut self, mut func: F) {
        self.buffer.push('\n');
        self.depth += 1;
        func(self);
        self.depth -= 1;
        self.buffer.push('\n');
    }

    fn indent(&mut self) {
        for _ in 0..(self.depth) {
            self.buffer.push_str("  ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate() {
        let mut obj = Object::new();

        obj.add("string", Json::String("foo\nbar".to_string()));
        obj.add("int", Json::Int(42));
        obj.add("array", Json::Array(vec![Json::Int(10), Json::Int(20)]));
        obj.add("bool", Json::Bool(true));

        let json = Generator::new().generate(&Json::Object(obj));

        assert_eq!(
            json,
            "{
  \"string\": \"foo\\nbar\",
  \"int\": 42,
  \"array\": [
    10,
    20
  ],
  \"bool\": true
}"
        );
    }

    #[test]
    fn test_escaped() {
        assert_eq!(escaped("foo"), "foo".to_string());
        assert_eq!(escaped("foo bar"), "foo bar".to_string());
        assert_eq!(escaped("a\nb"), "a\\nb".to_string());
        assert_eq!(escaped("a\rb"), "a\\rb".to_string());
        assert_eq!(escaped("a\tb"), "a\\tb".to_string());
    }
}

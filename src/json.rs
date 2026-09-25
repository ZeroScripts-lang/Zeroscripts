// Minimal JSON support for ZeroScript stdlib json.* (StandardLibrary.md §51).
// No external dependencies: hand-rolled recursive-descent parser + stringifier.

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!(
            "Unexpected trailing characters at position {}",
            p.pos
        ));
    }
    Ok(v)
}

pub fn stringify(value: &JsonValue) -> Result<String, String> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out)
}

fn write_value(v: &JsonValue, out: &mut String) -> Result<(), String> {
    match v {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(true) => out.push_str("true"),
        JsonValue::Bool(false) => out.push_str("false"),
        JsonValue::Number(n) => out.push_str(&format_number(*n)?),
        JsonValue::String(s) => write_string(s, out),
        JsonValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        JsonValue::Object(entries) => {
            out.push('{');
            for (i, (k, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(val, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn format_number(n: f64) -> Result<String, String> {
    if !n.is_finite() {
        return Err("Cannot stringify non-finite number".to_string());
    }
    if n == n.trunc() && n.abs() < 1e15 {
        Ok(format!("{}", n as i64))
    } else {
        Ok(format!("{}", n))
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        match self.peek() {
            Some(c) if c == b => {
                self.pos += 1;
                Ok(())
            }
            Some(c) => Err(format!(
                "Expected '{}' at position {}, found '{}'",
                b as char,
                self.pos,
                c as char
            )),
            None => Err(format!("Expected '{}' at end of input", b as char)),
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        match self.peek() {
            None => Err("Unexpected end of input".to_string()),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(JsonValue::String(self.parse_string()?)),
            Some(b't') => self.parse_lit("true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_lit("false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_lit("null", JsonValue::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(format!(
                "Unexpected character '{}' at position {}",
                c as char, self.pos
            )),
        }
    }

    fn parse_lit(&mut self, lit: &str, v: JsonValue) -> Result<JsonValue, String> {
        let lb = lit.as_bytes();
        if self.bytes[self.pos..].starts_with(lb) {
            self.pos += lb.len();
            Ok(v)
        } else {
            Err(format!(
                "Invalid literal at position {} (expected '{}')",
                self.pos, lit
            ))
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "Invalid number encoding".to_string())?;
        s.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| format!("Invalid number '{}'", s))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("Unterminated string".to_string()),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        None => return Err("Unterminated escape".to_string()),
                        Some(b'"') => {
                            out.push('"');
                            self.pos += 1;
                        }
                        Some(b'\\') => {
                            out.push('\\');
                            self.pos += 1;
                        }
                        Some(b'/') => {
                            out.push('/');
                            self.pos += 1;
                        }
                        Some(b'n') => {
                            out.push('\n');
                            self.pos += 1;
                        }
                        Some(b't') => {
                            out.push('\t');
                            self.pos += 1;
                        }
                        Some(b'r') => {
                            out.push('\r');
                            self.pos += 1;
                        }
                        Some(b'b') => {
                            out.push('\u{08}');
                            self.pos += 1;
                        }
                        Some(b'f') => {
                            out.push('\u{0c}');
                            self.pos += 1;
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let cp = self.parse_hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                // High surrogate: expect \uXXXX low surrogate
                                if self.peek() == Some(b'\\') {
                                    self.pos += 1;
                                    self.expect(b'u')?;
                                    let low = self.parse_hex4()?;
                                    if (0xDC00..0xE000).contains(&low) {
                                        let c = 0x10000
                                            + ((cp - 0xD800) << 10)
                                            + (low - 0xDC00);
                                        out.push(
                                            char::from_u32(c)
                                                .ok_or("Invalid surrogate pair")?,
                                        );
                                    } else {
                                        return Err("Invalid low surrogate".to_string());
                                    }
                                } else {
                                    return Err("Unpaired high surrogate".to_string());
                                }
                            } else if (0xDC00..0xE000).contains(&cp) {
                                return Err("Unpaired low surrogate".to_string());
                            } else {
                                out.push(
                                    char::from_u32(cp)
                                        .ok_or("Invalid unicode escape")?,
                                );
                            }
                        }
                        Some(c) => {
                            return Err(format!(
                                "Invalid escape '\\{}' at position {}",
                                c as char,
                                self.pos
                            ))
                        }
                    }
                }
                Some(_) => {
                    // Consume one UTF-8 character
                    let ch_start = self.pos;
                    let b = self.bytes[self.pos];
                    let len = if b < 0x80 {
                        1
                    } else if b >> 5 == 0b110 {
                        2
                    } else if b >> 4 == 0b1110 {
                        3
                    } else if b >> 3 == 0b11110 {
                        4
                    } else {
                        return Err(format!(
                            "Invalid UTF-8 at position {}",
                            self.pos
                        ));
                    };
                    if self.pos + len > self.bytes.len() {
                        return Err("Invalid UTF-8 (truncated)".to_string());
                    }
                    let s = std::str::from_utf8(&self.bytes[ch_start..self.pos + len])
                        .map_err(|_| "Invalid UTF-8 in string".to_string())?;
                    out.push_str(s);
                    self.pos += len;
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.bytes.len() {
            return Err("Truncated unicode escape".to_string());
        }
        let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| "Invalid unicode escape".to_string())?;
        let cp = u32::from_str_radix(hex, 16)
            .map_err(|_| format!("Invalid unicode escape '\\u{}'", hex))?;
        self.pos += 4;
        Ok(cp)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                Some(c) => {
                    return Err(format!(
                        "Expected ',' or ']' at position {}, found '{}'",
                        self.pos,
                        c as char
                    ))
                }
                None => return Err("Unterminated array".to_string()),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut entries: Vec<(String, JsonValue)> = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(entries));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "Expected string key at position {}",
                    self.pos
                ));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.parse_value()?;
            // Last-wins on duplicate keys (common JSON parser behavior).
            if let Some(slot) = entries.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = val;
            } else {
                entries.push((key, val));
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(entries));
                }
                Some(c) => {
                    return Err(format!(
                        "Expected ',' or '}}' at position {}, found '{}'",
                        self.pos,
                        c as char
                    ))
                }
                None => return Err("Unterminated object".to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scalars() {
        assert_eq!(parse("null").unwrap(), JsonValue::Null);
        assert_eq!(parse("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse("false").unwrap(), JsonValue::Bool(false));
        assert_eq!(parse("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(parse("-3.5").unwrap(), JsonValue::Number(-3.5));
        assert_eq!(parse("1e3").unwrap(), JsonValue::Number(1000.0));
        assert_eq!(
            parse("\"hi\"").unwrap(),
            JsonValue::String("hi".to_string())
        );
    }

    #[test]
    fn parse_nested() {
        let v = parse(r#"{"a":[1,2],"b":{"c":true}}"#).unwrap();
        match v {
            JsonValue::Object(entries) => assert_eq!(entries.len(), 2),
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn parse_escapes() {
        assert_eq!(
            parse(r#""line\nbreak""#).unwrap(),
            JsonValue::String("line\nbreak".to_string())
        );
        assert_eq!(
            parse(r#""A""#).unwrap(),
            JsonValue::String("A".to_string())
        );
    }

    #[test]
    fn parse_invalid() {
        assert!(parse("").is_err());
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("\"unterminated").is_err());
        assert!(parse("true false").is_err());
        assert!(parse("nul").is_err());
    }

    #[test]
    fn stringify_roundtrip() {
        let src = r#"{"name":"Kael","tags":["a","b"],"n":1,"ok":true,"nil":null}"#;
        let v = parse(src).unwrap();
        let s = stringify(&v).unwrap();
        let v2 = parse(&s).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn stringify_number_integer_form() {
        assert_eq!(
            stringify(&JsonValue::Number(10.0)).unwrap(),
            "10"
        );
        assert_eq!(
            stringify(&JsonValue::Number(3.14)).unwrap(),
            "3.14"
        );
    }

    #[test]
    fn stringify_string_escapes() {
        assert_eq!(
            stringify(&JsonValue::String("a\"b\\c\nd".to_string())).unwrap(),
            "\"a\\\"b\\\\c\\nd\""
        );
    }
}

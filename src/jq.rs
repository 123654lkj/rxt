//! JSON 查询/格式化 — 类似 jq (迷你版)
//!
//! 支持:
//! - 路径选择: `.`, `.foo`, `.foo.bar`, `.foo[0]`, `.[0]`, `.[2:5]`, `.[].foo`, `.[]`, `..`, `..|`
//! - pipe: `a | b`
//! - 字面量: `1`, `"hello"`, `true`, `false`, `null`, `[1,2,3]`, `{a: 1, b: .x}`
//! - 内置函数: `length`, `keys`, `values`, `type`, `select(EXPR)`, `map(EXPR)`, `not`, `has(KEY)`, `contains(EXPR)`, `if-then-else-end`, `unique`, `sort`, `sort_by`, `reverse`, `first`, `last`, `nth(n)`, `limit(n)`, `add`, `empty`, `ascii_downcase`, `ascii_upcase`, `tostring`, `tonumber`
//! - 比较: `==`, `!=`, `>`, `<`, `>=`, `<=`
//! - 逻辑: `and`, `or`, `not`
//! - 数学: `+`, `-`, `*`, `/`, `%`
//! - 输出格式器: `@csv`, `@json`, `@text`, `@tsv`, `@uri`, `@base64`, `@html`
//! - identity: `.`

use std::io::Read;
use serde_json::{Value, Number};

pub fn run(query: Option<&str>, file: Option<&std::path::Path>, fmt: bool, compact: bool, raw: bool, slurp: bool) -> anyhow::Result<()> {
    let inputs: Vec<Value> = if slurp {
        collect_inputs(file, true)?
    } else if let Some(f) = file {
        vec![serde_json::from_str(&std::fs::read_to_string(f)?)?]
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            anyhow::bail!("No input: pipe JSON or use -f <file>");
        }
        match serde_json::from_str(&buf) {
            Ok(v) => vec![v],
            Err(_) => {
                // JSONL fallback
                let mut all = Vec::new();
                for line in buf.lines() {
                    if line.trim().is_empty() { continue; }
                    all.push(serde_json::from_str(line)
                        .map_err(|e| anyhow::anyhow!("Not valid JSON or JSONL: {}", e))?);
                }
                if all.is_empty() { anyhow::bail!("No valid JSON found in input"); }
                all
            }
        }
    };

    if let Some(q) = query {
        let program = parse(q).map_err(|e| anyhow::anyhow!("parse error: {}", e))?;
        let mut outputs: Vec<Value> = Vec::new();
        for input in &inputs {
            outputs.extend(eval(&program, input));
        }
        if outputs.is_empty() { return Ok(()); }
        for v in outputs {
            if raw { print_raw(&v); }
            else if compact { println!("{}", serde_json::to_string(&v)?); }
            else { println!("{}", serde_json::to_string_pretty(&v)?); }
        }
    } else if fmt {
        for v in &inputs { println!("{}", serde_json::to_string_pretty(v)?); }
    } else if compact {
        for v in &inputs { println!("{}", serde_json::to_string(v)?); }
    } else {
        for v in &inputs { println!("{}", serde_json::to_string_pretty(v)?); }
    }
    Ok(())
}

fn collect_inputs(file: Option<&std::path::Path>, slurp: bool) -> anyhow::Result<Vec<Value>> {
    let buf = if let Some(f) = file { std::fs::read_to_string(f)? }
    else { let mut b = String::new(); std::io::stdin().read_to_string(&mut b)?; b };
    let mut all = Vec::new();
    if slurp {
        all.push(serde_json::from_str(&buf)?);
        return Ok(all);
    }
    for line in buf.lines() {
        if line.trim().is_empty() { continue; }
        all.push(serde_json::from_str(line)?);
    }
    Ok(all)
}

fn print_raw(v: &Value) {
    match v {
        Value::String(s) => print!("{}", s),
        Value::Number(n) => print!("{}", n),
        Value::Bool(b) => print!("{}", b),
        Value::Null => (),
        _ => println!("{}", serde_json::to_string(v).unwrap_or_default()),
    }
}

// ─────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Expr {
    Identity,
    Path(Vec<Segment>),
    Pipe(Box<Expr>, Box<Expr>),
    Literal(Value),
    Field(String),                 // bare identifier (becomes .identifier path)
    FunCall(String, Vec<Expr>),    // function call
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    IfThenElse(Box<Expr>, Box<Expr>, Box<Expr>),
    Array(Vec<Expr>),
    Object(Vec<(Expr, Expr)>),
    Comparator(Comparator, Box<Expr>, Box<Expr>),
    Comma(Box<Expr>, Box<Expr>),   // tuple output
    Try(Box<Expr>),               // suppress errors
    Format(String, Box<Expr>),    // @csv, @json, etc.
    Empty,                        // emits nothing
}

#[derive(Debug, Clone)]
enum Segment { Field(String), Index(usize), Slice(Option<i64>, Option<i64>), Iter, Recurse }

#[derive(Debug, Clone, Copy)] enum BinOp { Add, Sub, Mul, Div, Mod, And, Or }
#[derive(Debug, Clone, Copy)] enum UnaryOp { Neg, Not }
#[derive(Debug, Clone, Copy)] enum Comparator { Eq, Ne, Lt, Le, Gt, Ge }

impl Comparator {
    fn truthy(&self, a: &Value, b: &Value) -> bool {
        match self {
            Comparator::Eq => values_eq(a, b),
            Comparator::Ne => !values_eq(a, b),
            Comparator::Lt => cmp_values(a, b) == Some(std::cmp::Ordering::Less),
            Comparator::Le => matches!(cmp_values(a, b), Some(o) if o != std::cmp::Ordering::Greater),
            Comparator::Gt => cmp_values(a, b) == Some(std::cmp::Ordering::Greater),
            Comparator::Ge => matches!(cmp_values(a, b), Some(o) if o != std::cmp::Ordering::Less),
        }
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_eq(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).map_or(false, |yv| values_eq(v, yv))),
        _ => false,
    }
}

fn cmp_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64()?;
            let yf = y.as_f64()?;
            xf.partial_cmp(&yf)
        }
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────

const BUILTINS: &[&str] = &[
    "length", "keys", "keys_unsorted", "values", "type", "not",
    "select", "map", "has", "contains", "unique", "sort", "sort_by",
    "reverse", "first", "last", "nth", "limit", "add", "empty",
    "ascii_downcase", "ascii_upcase", "tostring", "tonumber",
    "to_entries", "from_entries", "with_entries",
    "min", "max", "flatten", "flatten_deep",
    "group_by", "min_by", "max_by",
    "recurse", "recurse_down", "walk",
];

struct Parser<'a> { src: &'a [u8], pos: usize }

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self { Self { src: src.as_bytes(), pos: 0 } }

    fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }
    fn advance(&mut self) -> Option<u8> { let c = self.peek()?; self.pos += 1; Some(c) }
    fn at_end(&self) -> bool { self.pos >= self.src.len() }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' { self.pos += 1; } else { break; }
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some(c) { self.pos += 1; Ok(()) }
        else { Err(format!("expected '{}', got {:?}", c as char, self.peek().map(|b| b as char))) }
    }

    fn try_consume(&mut self, c: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) { self.pos += 1; true } else { false }
    }

    fn peek_keyword(&self, kw: &str) -> bool {
        if self.pos + kw.len() > self.src.len() { return false; }
        if &self.src[self.pos..self.pos + kw.len()] != kw.as_bytes() { return false; }
        let after = self.src.get(self.pos + kw.len()).copied();
        match after {
            Some(c) => !(c.is_ascii_alphanumeric() || c == b'_'),
            None => true,
        }
    }

    fn expect_word(&mut self, kw: &str) -> Result<(), String> {
        self.skip_ws();
        if !self.peek_keyword(kw) {
            return Err(format!("expected keyword '{}'", kw));
        }
        self.pos += kw.len();
        Ok(())
    }

    fn parse_program(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        let expr = self.parse_pipe()?;
        self.skip_ws();
        if !self.at_end() {
            return Err(format!("unexpected trailing input at pos {}", self.pos));
        }
        Ok(expr)
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comma()?;
        loop {
            self.skip_ws();
            if self.try_consume(b'|') {
                let right = self.parse_pipe()?;
                left = Expr::Pipe(Box::new(left), Box::new(right));
            } else { break; }
        }
        Ok(left)
    }

    fn parse_comma(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_alt()?;
        loop {
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.pos += 1;
                let right = self.parse_comma()?;
                left = Expr::Comma(Box::new(left), Box::new(right));
            } else { break; }
        }
        Ok(left)
    }

    fn parse_alt(&mut self) -> Result<Expr, String> { self.parse_or() }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.peek_keyword("or") {
                self.pos += 2;
                let right = self.parse_or()?;
                left = Expr::BinOp(BinOp::Or, Box::new(left), Box::new(right));
            } else { break; }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        loop {
            self.skip_ws();
            if self.peek_keyword("and") {
                self.pos += 3;
                let right = self.parse_and()?;
                left = Expr::BinOp(BinOp::And, Box::new(left), Box::new(right));
            } else { break; }
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let left = self.parse_compare()?;
        self.skip_ws();
        if self.peek() == Some(b'=') && self.src.get(self.pos + 1) == Some(&b'=') {
            self.pos += 2;
            let right = self.parse_equality()?;
            return Ok(Expr::Comparator(Comparator::Eq, Box::new(left), Box::new(right)));
        }
        if self.peek() == Some(b'!') && self.src.get(self.pos + 1) == Some(&b'=') {
            self.pos += 2;
            let right = self.parse_equality()?;
            return Ok(Expr::Comparator(Comparator::Ne, Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr, String> {
        let left = self.parse_add()?;
        self.skip_ws();
        let op = match (self.peek(), self.src.get(self.pos + 1)) {
            (Some(b'<'), Some(b'=')) => Some(Comparator::Le),
            (Some(b'>'), Some(b'=')) => Some(Comparator::Ge),
            (Some(b'<'), _) => Some(Comparator::Lt),
            (Some(b'>'), _) => Some(Comparator::Gt),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += if matches!(op, Comparator::Le | Comparator::Ge) { 2 } else { 1 };
            let right = self.parse_compare()?;
            return Ok(Expr::Comparator(op, Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => { self.pos += 1; let right = self.parse_mul()?; left = Expr::BinOp(BinOp::Add, Box::new(left), Box::new(right)); }
                Some(b'-') => {
                    self.pos += 1;
                    let right = self.parse_mul()?;
                    left = Expr::BinOp(BinOp::Sub, Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => { self.pos += 1; let right = self.parse_unary()?; left = Expr::BinOp(BinOp::Mul, Box::new(left), Box::new(right)); }
                Some(b'/') => { self.pos += 1; let right = self.parse_unary()?; left = Expr::BinOp(BinOp::Div, Box::new(left), Box::new(right)); }
                Some(b'%') => { self.pos += 1; let right = self.parse_unary()?; left = Expr::BinOp(BinOp::Mod, Box::new(left), Box::new(right)); }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.peek() == Some(b'-') {
            if let Some(b'0'..=b'9') = self.src.get(self.pos + 1).copied() {
                self.pos += 1;
                let atom = self.parse_atom()?;
                return Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(atom)));
            }
        }
        if self.peek_keyword("not") {
            self.pos += 3;
            let atom = self.parse_unary()?;
            return Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(atom)));
        }
        self.parse_atom_with_postfix()
    }

    fn parse_atom_with_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_atom()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'.') => {
                    if self.src.get(self.pos + 1) == Some(&b'.') {
                        self.pos += 2;
                        expr = append_recurse(expr);
                        continue;
                    }
                    self.pos += 1;
                    self.skip_ws();
                    if self.peek() == Some(b'[') {
                        let seg = self.parse_index_inner()?;
                        expr = append_segment(expr, seg);
                    } else if self.peek() == Some(b'.') {
                        self.pos += 1;
                        expr = append_recurse(expr);
                    } else {
                        let key = self.read_ident()?;
                        if key.is_empty() {
                            return Err(format!("expected field name after '.' at pos {}", self.pos));
                        }
                        expr = append_segment(expr, Segment::Field(key));
                    }
                }
                Some(b'[') => {
                    self.pos += 1;
                    let seg = self.parse_index_inner()?;
                    expr = append_segment(expr, seg);
                }
                Some(b'?') => {
                    self.pos += 1;
                    expr = Expr::Try(Box::new(expr));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_index_inner(&mut self) -> Result<Segment, String> {
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Segment::Iter);
        }
        let start = self.parse_number_opt_i64()?;
        self.skip_ws();
        if self.peek() == Some(b':') {
            self.pos += 1;
            self.skip_ws();
            let end = if self.peek() == Some(b']') { None } else { Some(self.parse_number_opt_i64()?)};
            self.expect(b']')?;
            Ok(Segment::Slice(Some(start), end))
        } else {
            self.expect(b']')?;
            if start < 0 { return Err("negative index not supported (use slice [n:m])".to_string()); }
            Ok(Segment::Index(start as usize))
        }
    }

    fn parse_number_opt_i64(&mut self) -> Result<i64, String> {
        let neg = self.peek() == Some(b'-');
        if neg { self.pos += 1; }
        let start = self.pos;
        let mut has_digit = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() { self.pos += 1; has_digit = true; } else { break; }
        }
        if !has_digit {
            return Err(format!("expected number at pos {}", start));
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).map_err(|e| e.to_string())?;
        let n: i64 = s.parse().map_err(|e| format!("invalid number '{}': {}", s, e))?;
        Ok(if neg { -n } else { n })
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.pos += 1;
            let inner = self.parse_pipe()?;
            self.expect(b')')?;
            return Ok(inner);
        }
        if self.peek() == Some(b'[') {
            self.pos += 1;
            self.skip_ws();
            let mut items = Vec::new();
            if self.peek() != Some(b']') {
                items.push(self.parse_pipe()?);
                loop {
                    self.skip_ws();
                    if self.peek() == Some(b',') {
                        self.pos += 1;
                        items.push(self.parse_pipe()?);
                    } else { break; }
                }
            }
            self.expect(b']')?;
            return Ok(Expr::Array(items));
        }
        if self.peek() == Some(b'{') {
            self.pos += 1;
            let mut kvs = Vec::new();
            self.skip_ws();
            if self.peek() != Some(b'}') {
                loop {
                    self.skip_ws();
                    let key = self.parse_object_key()?;
                    self.skip_ws();
                    self.expect(b':')?;
                    let val = self.parse_pipe()?;
                    kvs.push((key, val));
                    self.skip_ws();
                    if self.peek() == Some(b',') { self.pos += 1; } else { break; }
                }
            }
            self.expect(b'}')?;
            return Ok(Expr::Object(kvs));
        }
        if self.peek() == Some(b'"') {
            return Ok(Expr::Literal(Value::String(self.parse_string()?)));
        }
        if let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                let n = self.parse_number_opt_i64()?;
                if n < 0 { return Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(Expr::Literal(Value::Number(Number::from((-n) as u64)))))); }
                return Ok(Expr::Literal(Value::Number(Number::from(n as u64))));
            }
        }
        if self.peek() == Some(b'.') {
            if self.src.get(self.pos + 1) == Some(&b'.') {
                self.pos += 2;
                return Ok(Expr::Path(vec![Segment::Recurse]));
            }
            self.pos += 1;
            self.skip_ws();
            if matches!(self.peek(), Some(b'.') | Some(b',') | Some(b']') | Some(b')') | Some(b'|') | Some(b'?') | None) {
                return Ok(Expr::Identity);
            }
            if self.peek() == Some(b'[') {
                let seg = self.parse_index_inner()?;
                return Ok(Expr::Path(vec![seg]));
            }
            let key = self.read_ident()?;
            if key.is_empty() { return Err(format!("expected field name at pos {}", self.pos)); }
            return Ok(Expr::Path(vec![Segment::Field(key)]));
        }
        if let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() || c == b'_' {
                let ident = self.read_ident()?;
                return self.parse_ident_or_call(&ident);
            }
        }
        if self.peek() == Some(b'@') {
            self.pos += 1;
            let name = self.read_ident()?;
            self.skip_ws();
            // @csv (no args) or @csv(EXPR)
            if self.peek() == Some(b'(') {
                self.pos += 1;
                let inner = self.parse_pipe()?;
                self.expect(b')')?;
                return Ok(Expr::Format(name, Box::new(inner)));
            } else {
                return Ok(Expr::Format(name, Box::new(Expr::Identity)));
            }
        }
        Err(format!("unexpected character '{}' at pos {}", self.peek().map(|b| b as char).unwrap_or('?'), self.pos))
    }

    fn parse_ident_or_call(&mut self, ident: &str) -> Result<Expr, String> {
        match ident {
            "true" => Ok(Expr::Literal(Value::Bool(true))),
            "false" => Ok(Expr::Literal(Value::Bool(false))),
            "null" => Ok(Expr::Literal(Value::Null)),
            "if" => self.parse_if(),
            _ => {
                if BUILTINS.contains(&ident) {
                    self.skip_ws();
                    if self.peek() == Some(b'(') {
                        self.pos += 1;
                        let mut args = Vec::new();
                        self.skip_ws();
                        if self.peek() != Some(b')') {
                            args.push(self.parse_pipe()?);
                            loop {
                                self.skip_ws();
                                if self.peek() == Some(b',') {
                                    self.pos += 1;
                                    args.push(self.parse_pipe()?);
                                } else { break; }
                            }
                        }
                        self.expect(b')')?;
                        Ok(Expr::FunCall(ident.to_string(), args))
                    } else {
                        Ok(Expr::FunCall(ident.to_string(), vec![]))
                    }
                } else {
                    // Bare identifier → becomes .identifier path
                    Ok(Expr::Field(ident.to_string()))
                }
            }
        }
    }

    fn parse_if(&mut self) -> Result<Expr, String> {
        let cond = self.parse_pipe()?;
        self.skip_ws();
        self.expect_word("then")?;
        let a = self.parse_pipe()?;
        self.skip_ws();
        self.expect_word("else")?;
        let b = self.parse_pipe()?;
        self.skip_ws();
        self.expect_word("end")?;
        Ok(Expr::IfThenElse(Box::new(cond), Box::new(a), Box::new(b)))
    }

    fn parse_object_key(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        if self.peek() == Some(b'"') {
            Ok(Expr::Literal(Value::String(self.parse_string()?)))
        } else if self.peek() == Some(b'(') {
            self.pos += 1;
            let inner = self.parse_pipe()?;
            self.expect(b')')?;
            Ok(inner)
        } else {
            let ident = self.read_ident()?;
            Ok(Expr::Literal(Value::String(ident)))
        }
    }

    fn read_ident(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' { self.pos += 1; } else { break; }
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .map(|s| s.to_string())
            .map_err(|e| e.to_string())
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(c) = self.advance() {
            if c == b'"' { return Ok(out); }
            if c == b'\\' {
                let esc = self.advance().ok_or_else(|| "unterminated string".to_string())?;
                match esc {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{08}'),
                    b'f' => out.push('\u{0C}'),
                    b'u' => {
                        let hex = self.read_n(4)?;
                        let cp = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                        if let Some(c) = char::from_u32(cp) { out.push(c); }
                        else { return Err(format!("invalid unicode \\u{}", hex)); }
                    }
                    _ => return Err(format!("unknown escape '\\{}'", esc as char)),
                }
            } else {
                out.push(c as char);
            }
        }
        Err("unterminated string".to_string())
    }

    fn read_n(&mut self, n: usize) -> Result<String, String> {
        let start = self.pos;
        self.pos += n;
        if self.pos > self.src.len() { return Err("unexpected end of string escape".to_string()); }
        std::str::from_utf8(&self.src[start..self.pos])
            .map(|s| s.to_string())
            .map_err(|e| e.to_string())
    }
}

fn append_segment(expr: Expr, seg: Segment) -> Expr {
    match expr {
        Expr::Path(mut segs) => { segs.push(seg); Expr::Path(segs) }
        Expr::Identity => Expr::Path(vec![seg]),
        other => Expr::Pipe(Box::new(other), Box::new(Expr::Path(vec![seg]))),
    }
}

fn append_recurse(expr: Expr) -> Expr {
    match expr {
        Expr::Path(mut segs) => { segs.push(Segment::Recurse); Expr::Path(segs) }
        Expr::Identity => Expr::Path(vec![Segment::Recurse]),
        other => Expr::Pipe(Box::new(other), Box::new(Expr::Path(vec![Segment::Recurse]))),
    }
}

fn num_arith<F: Fn(f64, f64) -> f64>(a: &Number, b: &Number, f: F) -> Value {
    let af = a.as_f64().unwrap_or(0.0);
    let bf = b.as_f64().unwrap_or(0.0);
    let r = f(af, bf);
    if r.fract() == 0.0 && r.is_finite() && r >= i64::MIN as f64 && r <= i64::MAX as f64 {
        Value::Number(Number::from(r as i64))
    } else {
        Number::from_f64(r).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn binop_apply(op: BinOp, a: &Value, b: &Value) -> Option<Value> {
    match op {
        BinOp::Add => match (a, b) {
            (Value::Number(x), Value::Number(y)) => Some(num_arith(x, y, |a,b| a+b)),
            (Value::String(x), Value::String(y)) => Some(Value::String(format!("{}{}", x, y))),
            (Value::Array(x), Value::Array(y)) => {
                let mut out = x.clone();
                out.extend(y.iter().cloned());
                Some(Value::Array(out))
            }
            (Value::Null, other) | (other, Value::Null) => Some(other.clone()),
            _ => None,
        },
        BinOp::Sub => match (a, b) {
            (Value::Number(x), Value::Number(y)) => Some(num_arith(x, y, |a,b| a-b)),
            (Value::Array(x), Value::Array(y)) => {
                // Set difference — keep order, dedupe y
                let mut out = Vec::new();
                for item in x {
                    if !y.iter().any(|y_item| values_eq(item, y_item)) {
                        out.push(item.clone());
                    }
                }
                Some(Value::Array(out))
            }
            _ => None,
        },
        BinOp::Mul => match (a, b) {
            (Value::Number(x), Value::Number(y)) => Some(num_arith(x, y, |a,b| a*b)),
            _ => None,
        },
        BinOp::Div => match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                if y.as_f64() == Some(0.0) { return None; }
                Some(num_arith(x, y, |a,b| a/b))
            }
            _ => None,
        },
        BinOp::Mod => match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                if y.as_f64() == Some(0.0) { return None; }
                Some(num_arith(x, y, |a,b| a%b))
            }
            _ => None,
        },
        BinOp::And | BinOp::Or => None, // handled by eval
    }
}

// ─────────────────────────────────────────────────────────────────
// Evaluator
// ─────────────────────────────────────────────────────────────────

fn eval(expr: &Expr, input: &Value) -> Vec<Value> {
    match expr {
        Expr::Identity => vec![input.clone()],
        Expr::Path(segs) => {
            let mut out = vec![input.clone()];
            for seg in segs {
                let mut next = Vec::new();
                for v in &out {
                    apply_segment(seg, v, &mut next);
                }
                out = next;
                if out.is_empty() { return out; }
            }
            out
        }
        Expr::Pipe(a, b) => {
            let mut out = Vec::new();
            for av in eval(a, input) { out.extend(eval(b, &av)); }
            out
        }
        Expr::Literal(v) => vec![v.clone()],
        Expr::Field(name) => {
            // Bare identifier in expression position — becomes .identifier lookup
            if let Value::Object(o) = input {
                if let Some(x) = o.get(name) { return vec![x.clone()]; }
            }
            vec![]
        }
        Expr::FunCall(name, args) => eval_func(name, args, input),
        Expr::BinOp(op, a, b) => {
            match op {
                BinOp::And => {
                    let mut out = Vec::new();
                    for av in eval(a, input) {
                        if !is_truthy(&av) { continue; }
                        for bv in eval(b, input) {
                            if is_truthy(&bv) {
                                out.push(av.clone());
                                break;
                            }
                        }
                    }
                    out
                }
                BinOp::Or => {
                    let mut out = Vec::new();
                    for av in eval(a, input) {
                        if is_truthy(&av) { out.push(av.clone()); continue; }
                        for bv in eval(b, input) {
                            if is_truthy(&bv) {
                                out.push(av.clone());
                                break;
                            }
                        }
                    }
                    out
                }
                _ => {
                    let mut out = Vec::new();
                    for av in eval(a, input) {
                        for bv in eval(b, input) {
                            if let Some(r) = binop_apply(*op, &av, &bv) {
                                out.push(r);
                            }
                        }
                    }
                    out
                }
            }
        }
        Expr::UnaryOp(op, e) => {
            let mut out = Vec::new();
            for v in eval(e, input) {
                match op {
                    UnaryOp::Neg => {
                        if let Value::Number(n) = &v {
                            if let Some(f) = n.as_f64() {
                                let zero = Number::from(0u64);
                                let nb = Number::from_f64(f).unwrap_or(Number::from(0u64));
                                out.push(num_arith(&zero, &nb, |a, b| a - b));
                            }
                        }
                    }
                    UnaryOp::Not => {
                        out.push(Value::Bool(!is_truthy(&v)));
                    }
                }
            }
            out
        }
        Expr::IfThenElse(c, t, e) => {
            let mut out = Vec::new();
            for cv in eval(c, input) {
                if is_truthy(&cv) { out.extend(eval(t, input)); }
                else { out.extend(eval(e, input)); }
            }
            out
        }
        Expr::Array(items) => {
            // jq semantics: [expr1, expr2, ...] collects outputs.
            // If single expr with multiple values: collect them into one array.
            // If multiple exprs: each becomes one element; multiple outputs of any expr
            //   trigger Cartesian product across all exprs (jq behavior).
            if items.len() == 1 {
                let vals = eval(&items[0], input);
                vec![Value::Array(vals)]
            } else {
                // Cartesian across each item's outputs
                let mut results = vec![Vec::new()];
                for item in items {
                    let mut next = Vec::new();
                    for prefix in &results {
                        for v in eval(item, input) {
                            let mut p = prefix.clone();
                            p.push(v);
                            next.push(p);
                        }
                    }
                    results = next;
                }
                results.into_iter().map(|v| Value::Array(v)).collect()
            }
        }
        Expr::Object(kvs) => {
            let mut results = vec![Vec::new()];
            for (k, v) in kvs {
                let mut next = Vec::new();
                for prefix in &results {
                    let key_vals: Vec<Value> = eval(k, input);
                    let val_vals: Vec<Value> = eval(v, input);
                    let key_vals = if key_vals.is_empty() { vec![Value::Null] } else { key_vals };
                    for kk in &key_vals {
                        for vv in &val_vals {
                            let mut p = prefix.clone();
                            p.push((kk.clone(), vv.clone()));
                            next.push(p);
                        }
                    }
                }
                results = next;
            }
            let mut out = Vec::new();
            for pairs in results {
                let mut obj = serde_json::Map::new();
                for (k, v) in pairs {
                    let key = match k {
                        Value::String(s) => s,
                        _ => serde_json::to_string(&k).unwrap_or_default(),
                    };
                    obj.insert(key, v);
                }
                out.push(Value::Object(obj));
            }
            out
        }
        Expr::Comparator(op, a, b) => {
            let mut out = Vec::new();
            for av in eval(a, input) {
                for bv in eval(b, input) {
                    if op.truthy(&av, &bv) {
                        out.push(av.clone());
                    }
                }
            }
            out
        }
        Expr::Comma(a, b) => {
            let mut out = eval(a, input);
            out.extend(eval(b, input));
            out
        }
        Expr::Try(e) => eval(e, input),
        Expr::Format(name, e) => {
            let mut out = Vec::new();
            for v in eval(e, input) {
                if let Some(s) = apply_format(name, &v) {
                    out.push(Value::String(s));
                }
            }
            out
        }
        Expr::Empty => vec![],
    }
}

fn apply_segment(seg: &Segment, v: &Value, out: &mut Vec<Value>) {
    match seg {
        Segment::Field(k) => {
            if let Value::Object(o) = v {
                if let Some(x) = o.get(k) { out.push(x.clone()); }
            }
        }
        Segment::Index(i) => {
            if let Value::Array(a) = v {
                if *i < a.len() { out.push(a[*i].clone()); }
            }
        }
        Segment::Slice(s, e) => {
            if let Value::Array(a) = v {
                let len = a.len() as i64;
                let s = s.unwrap_or(0).max(0).min(len) as usize;
                let e = e.unwrap_or(len).max(0).min(len) as usize;
                if s <= e { out.push(Value::Array(a[s..e].to_vec())); }
            }
        }
        Segment::Iter => {
            if let Value::Array(a) = v { out.extend(a.iter().cloned()); }
            else if let Value::Object(o) = v { out.extend(o.values().cloned()); }
        }
        Segment::Recurse => {
            recurse_collect(v, out);
        }
    }
}

fn recurse_collect(v: &Value, out: &mut Vec<Value>) {
    out.push(v.clone());
    match v {
        Value::Array(a) => { for x in a { recurse_collect(x, out); } }
        Value::Object(o) => { for x in o.values() { recurse_collect(x, out); } }
        _ => {}
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(false, |f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn eval_func(name: &str, args: &[Expr], input: &Value) -> Vec<Value> {
    match name {
        "length" => {
            let n = match input {
                Value::Array(a) => a.len() as i64,
                Value::Object(o) => o.len() as i64,
                Value::String(s) => s.chars().count() as i64,
                Value::Null => 0,
                _ => 1,
            };
            vec![Value::Number(Number::from(n))]
        }
        "keys" => {
            if let Value::Object(o) = input {
                let mut keys: Vec<Value> = o.keys().map(|k| Value::String(k.clone())).collect();
                keys.sort_by(|a, b| {
                    if let (Value::String(x), Value::String(y)) = (a, b) { x.cmp(y) } else { std::cmp::Ordering::Equal }
                });
                vec![Value::Array(keys)]
            } else { vec![] }
        }
        "keys_unsorted" => {
            if let Value::Object(o) = input {
                let keys: Vec<Value> = o.keys().map(|k| Value::String(k.clone())).collect();
                vec![Value::Array(keys)]
            } else { vec![] }
        }
        "values" => {
            match input {
                Value::Object(o) => vec![Value::Array(o.values().cloned().collect())],
                Value::Array(a) => vec![Value::Array(a.clone())],
                _ => vec![],
            }
        }
        "type" => vec![Value::String(match input {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }.to_string())],
        "not" => vec![Value::Bool(!is_truthy(input))],
        "select" => {
            let mut out = Vec::new();
            for arg in args {
                let cond_results = eval(arg, input);
                if cond_results.iter().any(is_truthy) {
                    out.push(input.clone());
                }
            }
            out
        }
        "map" => {
            let mut out = Vec::new();
            if let Value::Array(a) = input {
                for item in a {
                    for arg in args {
                        out.extend(eval(arg, item));
                    }
                }
            }
            out
        }
        "has" => {
            let mut out = Vec::new();
            for arg in args {
                for key in eval(arg, input) {
                    let key_str = match key {
                        Value::String(s) => Some(s),
                        _ => None,
                    };
                    if let (Some(ks), Value::Object(o)) = (key_str, input) {
                        out.push(Value::Bool(o.contains_key(&ks)));
                    } else {
                        out.push(Value::Bool(false));
                    }
                }
            }
            out
        }
        "contains" => {
            let mut out = Vec::new();
            for arg in args {
                for needle in eval(arg, input) {
                    out.push(Value::Bool(json_contains(input, &needle)));
                }
            }
            out
        }
        "unique" => {
            if let Value::Array(a) = input {
                let mut seen: Vec<Value> = Vec::new();
                for item in a {
                    if !seen.iter().any(|x| values_eq(x, item)) {
                        seen.push(item.clone());
                    }
                }
                vec![Value::Array(seen)]
            } else { vec![input.clone()] }
        }
        "sort" => {
            if let Value::Array(a) = input {
                let mut sorted = a.clone();
                sorted.sort_by(|x, y| cmp_values(x, y).unwrap_or(std::cmp::Ordering::Equal));
                vec![Value::Array(sorted)]
            } else { vec![input.clone()] }
        }
        "sort_by" => {
            if let Value::Array(a) = input {
                let mut pairs: Vec<(Value, Value)> = Vec::new();
                for item in a {
                    let mut keys = Vec::new();
                    for arg in args {
                        keys.extend(eval(arg, item));
                    }
                    let key = keys.into_iter().next().unwrap_or(Value::Null);
                    pairs.push((key, item.clone()));
                }
                pairs.sort_by(|a, b| cmp_values(&a.0, &b.0).unwrap_or(std::cmp::Ordering::Equal));
                vec![Value::Array(pairs.into_iter().map(|p| p.1).collect())]
            } else { vec![input.clone()] }
        }
        "reverse" => {
            if let Value::Array(a) = input {
                let mut rev = a.clone();
                rev.reverse();
                vec![Value::Array(rev)]
            } else { vec![input.clone()] }
        }
        "first" => {
            if let Value::Array(a) = input {
                if let Some(f) = a.first() { return vec![f.clone()]; }
            }
            vec![]
        }
        "last" => {
            if let Value::Array(a) = input {
                if let Some(l) = a.last() { return vec![l.clone()]; }
            }
            vec![]
        }
        "nth" => {
            if let Value::Array(a) = input {
                for arg in args {
                    for n in eval(arg, input) {
                        if let Value::Number(idx) = n {
                            let i = idx.as_u64().unwrap_or(0) as usize;
                            if i < a.len() { return vec![a[i].clone()]; }
                        }
                    }
                }
            }
            vec![]
        }
        "limit" => {
            let mut n = usize::MAX;
            for arg in args {
                for v in eval(arg, input) {
                    if let Value::Number(num) = v {
                        n = num.as_u64().unwrap_or(0) as usize;
                    }
                }
            }
            if let Value::Array(a) = input {
                vec![Value::Array(a.iter().take(n).cloned().collect())]
            } else { vec![input.clone()] }
        }
        "add" => {
            let mut acc = input.clone();
            for arg in args {
                for v in eval(arg, input) {
                    acc = binop_apply(BinOp::Add, &acc, &v).unwrap_or(Value::Null);
                }
            }
            vec![acc]
        }
        "empty" => vec![Value::Bool(match input {
            Value::Null => true,
            Value::Array(a) => a.is_empty(),
            Value::Object(o) => o.is_empty(),
            Value::String(s) => s.is_empty(),
            _ => false,
        })],
        "ascii_downcase" => {
            if let Value::String(s) = input { vec![Value::String(s.to_ascii_lowercase())] }
            else { vec![input.clone()] }
        }
        "ascii_upcase" => {
            if let Value::String(s) = input { vec![Value::String(s.to_ascii_uppercase())] }
            else { vec![input.clone()] }
        }
        "tostring" => vec![Value::String(serde_json::to_string(input).unwrap_or_default())],
        "tonumber" => {
            if let Value::String(s) = input {
                if let Ok(n) = s.parse::<f64>() {
                    if let Some(num) = Number::from_f64(n) { return vec![Value::Number(num)]; }
                }
            }
            if let Value::Number(n) = input { return vec![Value::Number(n.clone())]; }
            vec![]
        }
        "to_entries" => {
            if let Value::Object(o) = input {
                let entries: Vec<Value> = o.iter().map(|(k, v)| {
                    let mut m = serde_json::Map::new();
                    m.insert("key".to_string(), Value::String(k.clone()));
                    m.insert("value".to_string(), v.clone());
                    Value::Object(m)
                }).collect();
                vec![Value::Array(entries)]
            } else { vec![] }
        }
        "from_entries" => {
            let mut out = Vec::new();
            for arg in args {
                for v in eval(arg, input) {
                    if let Value::Array(arr) = v {
                        let mut obj = serde_json::Map::new();
                        for item in arr {
                            if let Value::Object(o) = item {
                                if let Some(k) = o.get("key") {
                                    let key = match k {
                                        Value::String(s) => s.clone(),
                                        _ => serde_json::to_string(k).unwrap_or_default(),
                                    };
                                    let val = o.get("value").cloned().unwrap_or(Value::Null);
                                    obj.insert(key, val);
                                }
                            }
                        }
                        out.push(Value::Object(obj));
                    }
                }
            }
            out
        }
        "with_entries" => {
            let entries = eval_func("to_entries", &[], input);
            let mut out = Vec::new();
            for entry in entries {
                for arg in args {
                    for v in eval(arg, &entry) {
                        if let Value::Object(o) = v {
                            if let (Some(k), Some(val)) = (o.get("key"), o.get("value")) {
                                let key = match k {
                                    Value::String(s) => s.clone(),
                                    _ => serde_json::to_string(k).unwrap_or_default(),
                                };
                                let mut m = serde_json::Map::new();
                                m.insert(key, val.clone());
                                out.push(Value::Object(m));
                            }
                        }
                    }
                }
            }
            let mut merged = serde_json::Map::new();
            for v in out {
                if let Value::Object(o) = v {
                    for (k, val) in o { merged.insert(k, val); }
                }
            }
            vec![Value::Object(merged)]
        }
        "min" => {
            if let Value::Array(a) = input {
                a.iter().min_by(|x, y| cmp_values(x, y).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|v| vec![v.clone()]).unwrap_or_default()
            } else { vec![] }
        }
        "max" => {
            if let Value::Array(a) = input {
                a.iter().max_by(|x, y| cmp_values(x, y).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|v| vec![v.clone()]).unwrap_or_default()
            } else { vec![] }
        }
        "flatten" => {
            if let Value::Array(a) = input {
                let depth = if let Some(arg) = args.first() {
                    eval(arg, input).into_iter().next().and_then(|v| {
                        if let Value::Number(n) = v { n.as_i64() } else { None }
                    }).unwrap_or(1)
                } else { 1 };
                let mut out = Vec::new();
                flatten(input, depth, &mut out);
                vec![Value::Array(out)]
            } else { vec![input.clone()] }
        }
        "min_by" | "max_by" | "group_by" => {
            if let Value::Array(a) = input {
                let mut pairs: Vec<(Value, Value)> = Vec::new();
                for item in a {
                    let mut keys = Vec::new();
                    for arg in args {
                        keys.extend(eval(arg, item));
                    }
                    let key = keys.into_iter().next().unwrap_or(Value::Null);
                    pairs.push((key, item.clone()));
                }
                match name {
                    "min_by" => {
                        let min = pairs.iter().min_by(|x, y| cmp_values(&x.0, &y.0).unwrap_or(std::cmp::Ordering::Equal));
                        min.map(|p| vec![p.1.clone()]).unwrap_or_default()
                    }
                    "max_by" => {
                        let max = pairs.iter().max_by(|x, y| cmp_values(&x.0, &y.0).unwrap_or(std::cmp::Ordering::Equal));
                        max.map(|p| vec![p.1.clone()]).unwrap_or_default()
                    }
                    "group_by" => {
                        let mut groups: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
                        for (k, v) in pairs {
                            let key_str = match k {
                                Value::String(s) => s,
                                _ => serde_json::to_string(&k).unwrap_or_default(),
                            };
                            groups.entry(key_str).or_default().push(v);
                        }
                        let arr: Vec<Value> = groups.into_values().map(Value::Array).collect();
                        vec![Value::Array(arr)]
                    }
                    _ => vec![],
                }
            } else { vec![] }
        }
        "recurse" => {
            let mut out = Vec::new();
            recurse_collect(input, &mut out);
            out
        }
        "walk" => {
            fn walk(v: &Value) -> Value {
                match v {
                    Value::Array(a) => {
                        // First apply children (jq semantics: walk visits children before parent? actually jq walks after children)
                        // We'll follow jq: walk(f) applies f to all children, then to current
                        let new_items: Vec<Value> = a.iter().map(walk).collect();
                        // Then run user function on the new array
                        // For simplicity, no user func — just walk structure
                        Value::Array(new_items)
                    }
                    Value::Object(o) => {
                        let mut new_obj = serde_json::Map::new();
                        for (k, v) in o { new_obj.insert(k.clone(), walk(v)); }
                        Value::Object(new_obj)
                    }
                    other => other.clone(),
                }
            }
            vec![walk(input)]
        }
        _ => vec![],
    }
}

fn flatten(v: &Value, depth: i64, out: &mut Vec<Value>) {
    if depth == 0 { out.push(v.clone()); return; }
    if let Value::Array(a) = v {
        for x in a { flatten(x, depth - 1, out); }
    } else { out.push(v.clone()); }
}

fn json_contains(haystack: &Value, needle: &Value) -> bool {
    if values_eq(haystack, needle) { return true; }
    match (haystack, needle) {
        (Value::Array(h), Value::Array(n)) => {
            if n.is_empty() { return true; }
            let mut ni = 0;
            for h_item in h {
                if ni < n.len() && values_eq(h_item, &n[ni]) {
                    ni += 1;
                    if ni == n.len() { return true; }
                }
            }
            false
        }
        (Value::Object(h), Value::Object(n)) => {
            n.iter().all(|(k, v)| h.get(k).map_or(false, |hv| json_contains(hv, v)))
        }
        _ => false,
    }
}

fn apply_format(name: &str, v: &Value) -> Option<String> {
    match name {
        "csv" | "tsv" => {
            let sep = if name == "csv" { ',' } else { '\t' };
            if let Value::Array(a) = v {
                let row: Vec<String> = a.iter().map(|x| format_csv_field(x, sep)).collect();
                Some(row.join(&sep.to_string()))
            } else { Some(format_csv_field(v, sep)) }
        }
        "json" => Some(serde_json::to_string(v).unwrap_or_default()),
        "text" => Some(match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            _ => serde_json::to_string(v).unwrap_or_default(),
        }),
        "uri" => {
            let raw = match v {
                Value::String(s) => s.clone(),
                _ => serde_json::to_string(v).unwrap_or_default(),
            };
            let mut out = String::with_capacity(raw.len());
            for c in raw.chars() {
                if c.is_ascii_alphanumeric() || "-._~".contains(c) {
                    out.push(c);
                } else {
                    for b in c.to_string().as_bytes() {
                        out.push_str(&format!("%{:02X}", b));
                    }
                }
            }
            Some(out)
        }
        "base64" => {
            let raw = match v {
                Value::String(s) => s.as_bytes().to_vec(),
                _ => serde_json::to_vec(v).unwrap_or_default(),
            };
            Some(base64_encode(&raw))
        }
        "html" => {
            let s = match v {
                Value::String(s) => s.clone(),
                _ => serde_json::to_string(v).unwrap_or_default(),
            };
            Some(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"))
        }
        _ => None,
    }
}

fn format_csv_field(v: &Value, sep: char) -> String {
    match v {
        Value::String(s) => {
            if s.contains(sep) || s.contains('"') || s.contains('\n') {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else { s.clone() }
        }
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8) | (data[i+2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn parse(src: &str) -> Result<Expr, String> {
    let mut p = Parser::new(src);
    p.parse_program()
}
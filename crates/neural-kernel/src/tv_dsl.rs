//! TV-DSL Co-processor — Deterministic Math Expression Parser.
//! Executa expressoes matematicas sem alucinacao.
//! Formato: [TV-DSL: add(mul(x, 2), 5)]
//! Suporta: add, sub, mul, div, sin, cos, sqrt, pow, abs, neg

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

enum Token {
    Number(f32),
    Ident(&'static str),
    LParen,
    RParen,
    Comma,
    Eof,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Lexer { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        if self.pos >= self.input.len() { return Token::Eof; }

        let c = self.input.as_bytes()[self.pos];
        if c.is_ascii_digit() || c == b'-' && self.pos + 1 < self.input.len()
            && self.input.as_bytes()[self.pos + 1].is_ascii_digit() {
            let start = self.pos;
            if c == b'-' { self.pos += 1; }
            while self.pos < self.input.len() && (self.input.as_bytes()[self.pos].is_ascii_digit()
                || self.input.as_bytes()[self.pos] == b'.') {
                self.pos += 1;
            }
            let num: f32 = self.input[start..self.pos].parse().unwrap_or(0.0);
            return Token::Number(num);
        }

        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.pos;
            while self.pos < self.input.len() && (self.input.as_bytes()[self.pos].is_ascii_alphanumeric()
                || self.input.as_bytes()[self.pos] == b'_') {
                self.pos += 1;
            }
            let ident = &self.input[start..self.pos];
            let leaked: &'static str = Box::leak(String::from(ident).into_boxed_str());
            return Token::Ident(leaked);
        }

        self.pos += 1;
        match c {
            b'(' => Token::LParen,
            b')' => Token::RParen,
            b',' => Token::Comma,
            _ => Token::Eof,
        }
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token();
        Parser { lexer, current }
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
    }

    fn parse_expr(&mut self) -> Result<f32, &'static str> {
        match self.current {
            Token::Number(n) => {
                let val = n;
                self.advance();
                Ok(val)
            }
            Token::Ident(name) => {
                let func_name = name;
                self.advance();
                if !matches!(self.current, Token::LParen) {
                    return Ok(0.0);
                }
                self.advance();
                let mut args = Vec::new();
                loop {
                    args.push(self.parse_expr()?);
                    match self.current {
                        Token::Comma => { self.advance(); }
                        Token::RParen => { self.advance(); break; }
                        _ => return Err("Expected ',' or ')' in function call"),
                    }
                }
                apply_function(func_name, &args)
            }
            Token::LParen => {
                self.advance();
                let val = self.parse_expr()?;
                if !matches!(self.current, Token::RParen) {
                    return Err("Expected ')'");
                }
                self.advance();
                Ok(val)
            }
            _ => Err("Unexpected token in expression"),
        }
    }

    fn parse(&mut self) -> Result<f32, &'static str> {
        let result = self.parse_expr()?;
        if !matches!(self.current, Token::Eof) {
            return Err("Unexpected token after expression");
        }
        Ok(result)
    }
}

fn apply_function(name: &str, args: &[f32]) -> Result<f32, &'static str> {
    match name {
        "add" | "sum" => Ok(args.iter().sum()),
        "sub" | "subtract" => {
            if args.len() == 2 { Ok(args[0] - args[1]) }
            else { Err("sub requires 2 arguments") }
        }
        "mul" | "multiply" | "product" => Ok(args.iter().product()),
        "div" | "divide" => {
            if args.len() == 2 {
                if args[1] == 0.0 { Err("Division by zero") }
                else { Ok(args[0] / args[1]) }
            } else { Err("div requires 2 arguments") }
        }
        "sin" => {
            if args.len() == 1 { Ok(unsafe { libm::sinf(args[0]) }) }
            else { Err("sin requires 1 argument") }
        }
        "cos" => {
            if args.len() == 1 { Ok(unsafe { libm::cosf(args[0]) }) }
            else { Err("cos requires 1 argument") }
        }
        "sqrt" => {
            if args.len() == 1 {
                if args[0] < 0.0 { Err("sqrt of negative") }
                else { Ok(unsafe { libm::sqrtf(args[0]) }) }
            } else { Err("sqrt requires 1 argument") }
        }
        "pow" | "power" => {
            if args.len() == 2 { Ok(unsafe { libm::powf(args[0], args[1]) }) }
            else { Err("pow requires 2 arguments") }
        }
        "abs" | "absolute" => {
            if args.len() == 1 { Ok(if args[0] < 0.0 { -args[0] } else { args[0] }) }
            else { Err("abs requires 1 argument") }
        }
        "neg" | "negative" => {
            if args.len() == 1 { Ok(-args[0]) }
            else { Err("neg requires 1 argument") }
        }
        "max" => {
            if args.is_empty() { Err("max requires at least 1 argument") }
            else {
                let mut m = args[0];
                for &v in &args[1..] { if v > m { m = v; } }
                Ok(m)
            }
        }
        "min" => {
            if args.is_empty() { Err("min requires at least 1 argument") }
            else {
                let mut m = args[0];
                for &v in &args[1..] { if v < m { m = v; } }
                Ok(m)
            }
        }
        "pi" => Ok(core::f32::consts::PI),
        "e" => Ok(core::f32::consts::E),
        "ceil" => {
            if args.len() == 1 { Ok(unsafe { libm::ceilf(args[0]) }) }
            else { Err("ceil needs 1 arg") }
        }
        "floor" => {
            if args.len() == 1 { Ok(unsafe { libm::floorf(args[0]) }) }
            else { Err("floor needs 1 arg") }
        }
        "round" => {
            if args.len() == 1 { Ok(unsafe { libm::roundf(args[0]) }) }
            else { Err("round needs 1 arg") }
        }
        "log" | "ln" => {
            if args.len() == 1 {
                if args[0] <= 0.0 { Err("log of non-positive") }
                else { Ok(unsafe { libm::logf(args[0]) }) }
            } else { Err("log requires 1 argument") }
        }
        "log10" => {
            if args.len() == 1 {
                if args[0] <= 0.0 { Err("log10 of non-positive") }
                else { Ok(unsafe { libm::log10f(args[0]) }) }
            } else { Err("log10 requires 1 argument") }
        }
        "tan" => {
            if args.len() == 1 { Ok(unsafe { libm::tanf(args[0]) }) }
            else { Err("tan needs 1 arg") }
        }
        _ => Err("Unknown function"),
    }
}

pub fn parse_tv_dsl(expr: &str) -> Result<f32, &'static str> {
    let mut parser = Parser::new(expr);
    parser.parse()
}

pub fn scan_and_execute(text: &str) -> Result<String, &'static str> {
    let marker_start = "[TV-DSL: ";
    let marker_end = "]";
    let mut result = String::new();
    let mut search_pos = 0;

    loop {
        let remaining = &text[search_pos..];
        if let Some(start) = remaining.find(marker_start) {
            result.push_str(&remaining[..start]);
            let inner_start = search_pos + start + marker_start.len();
            if let Some(end) = text[inner_start..].find(marker_end) {
                let expr = &text[inner_start..inner_start + end];
                match parse_tv_dsl(expr) {
                    Ok(val) => result.push_str(&format_float(val)),
                    Err(e) => result.push_str(&alloc::format!("[TV-DSL Error: {}]", e)),
                }
                search_pos = inner_start + end + marker_end.len();
            } else {
                result.push_str(&remaining[start..]);
                break;
            }
        } else {
            result.push_str(remaining);
            break;
        }
    }

    Ok(result)
}

fn format_float(f: f32) -> String {
    let truncated = unsafe { libm::truncf(f) };
    if (f - truncated).abs() < 0.0001 && f.is_finite() {
        alloc::format!("{:.0}", f)
    } else {
        alloc::format!("{:.4}", f)
    }
}

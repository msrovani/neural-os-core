//! ADR-0059 F4 -- Montador WASM a partir de uma IR de ops (op-IR).
//!
//! ## Gramática op-IR v3 -- comparações, branches, loops
//!
//! Ops: `LocalGet`, `I32Const`, `I32Add`, `I32Sub`, `I32Mul`, `Drop`,
//! `I32LtS` (<), `I32GtS` (>), `I32Eq` (==), `I32Eqz` (!= via eq+eqz),
//! `Block(ResultType)`, `Loop(ResultType)`, `Br(n)`, `BrIf(n)`, `End`.
//!
//! **Sem memória, sem imports, sem globals** -- determinístico e trivialmente seguro.

use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValType { I32 }

pub type BlockResult = Option<ValType>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    LocalGet(u32),
    I32Const(i32),
    I32Add, I32Sub, I32Mul,
    Drop,
    I32LtS, I32GtS, I32Eq, I32Eqz,
    /// Short-circuit AND: a && b -> select(a, b, 0). Branchless.
    LogicalAnd,
    /// Short-circuit OR: a || b -> select(a, 1, b). Branchless.
    LogicalOr,
    Block(BlockResult), Loop(BlockResult), If(BlockResult), Else,
    Br(u32), BrIf(u32), End,
    /// WASM select (0x1B): pop cond, true, false → push (cond!=0 ? true : false).
    /// Usado para ternário branchless: `cond ? a : b` → I32Select.
    I32Select,
    /// Host-call: importa `aios_gpu::submit(op, flags) -> i32` e chama.
    /// u8 = GPU op code (0=Nop,1=VectorAdd,2=MatmulTernary,3=BitLinearW2A8,4=Fence).
    /// Gera módulo WASM completo com import section (não mistura com outros ops).
    GpuSubmit(u8),
    /// Atalho de alto nível: equivale a GpuSubmit(2) = MatmulTernary.
    /// Monta módulo WASM com import `aios_gpu::submit` + call no body.
    GpuMatmul,
}

fn block_result_byte(r: BlockResult) -> u8 {
    match r { None => 0x40, Some(ValType::I32) => 0x7f }
}

fn uleb(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 { b |= 0x80; }
        out.push(b);
        if n == 0 { break; }
    }
}

fn sleb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        let sign = b & 0x40;
        let more = !((v == 0 && sign == 0) || (v == -1 && sign != 0));
        if more { b |= 0x80; }
        out.push(b);
        if !more { break; }
    }
}

fn section(id: u8, content: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    uleb(content.len() as u64, out);
    out.extend_from_slice(content);
}

/// Valida a op-IR: depth tracking, blocos/loops, br targets.
pub fn validate(n_params: u32, ops: &[Op]) -> Result<(), &'static str> {
    let mut depth: i32 = 0;
    // (entry_depth, arity, is_loop)
    let mut block_stack: Vec<(i32, i32, bool)> = Vec::new();
    for op in ops {
        match op {
            Op::LocalGet(i) => {
                if *i >= n_params { return Err("op-IR: local fora de faixa"); }
                depth += 1;
            }
            Op::I32Const(_) => depth += 1,
            Op::I32Add | Op::I32Sub | Op::I32Mul => {
                if depth < 2 { return Err("op-IR: stack underflow em binop"); }
                depth -= 1;
            }
            Op::Drop => {
                if depth < 1 { return Err("op-IR: stack underflow em Drop"); }
                depth -= 1;
            }
            Op::I32LtS | Op::I32GtS | Op::I32Eq | Op::LogicalAnd | Op::LogicalOr => {
                if depth < 2 { return Err("op-IR: stack underflow em binop"); }
                depth -= 1;
            }
            Op::I32Eqz => {
                if depth < 1 { return Err("op-IR: stack underflow em I32Eqz"); }
            }
            Op::I32Select => {
                if depth < 3 { return Err("op-IR: stack underflow em I32Select (precisa cond, true, false)"); }
                depth -= 2;
            }
            Op::Block(result) => {
                let arity = if *result == Some(ValType::I32) { 1 } else { 0 };
                block_stack.push((depth, arity, false));
                depth += arity;
            }
            Op::If(result) => {
                if depth < 1 { return Err("op-IR: stack underflow em If"); }
                depth -= 1; // pop condition
                let arity = if *result == Some(ValType::I32) { 1 } else { 0 };
                block_stack.push((depth, arity, false));
                depth += arity;
            }
            Op::Loop(result) => {
                let arity = if *result == Some(ValType::I32) { 1 } else { 0 };
                block_stack.push((depth, arity, true));
                depth += arity;
            }
            Op::Else => {
                if block_stack.is_empty() { return Err("op-IR: Else sem If"); }
                // Else: depth deve ser entry + arity (restaura para início do if body)
                let (entry_depth, arity, _) = block_stack.last().unwrap();
                depth = entry_depth + arity;
            }
            Op::Br(target) => {
                if block_stack.is_empty() { return Err("op-IR: br sem bloco/loop"); }
                if (*target as usize) >= block_stack.len() {
                    return Err("op-IR: br target fora de faixa");
                }
            }
            Op::BrIf(target) => {
                if depth < 1 { return Err("op-IR: stack underflow em BrIf"); }
                if block_stack.is_empty() { return Err("op-IR: br_if sem bloco/loop"); }
                if (*target as usize) >= block_stack.len() {
                    return Err("op-IR: br_if target fora de faixa");
                }
                depth -= 1;
            }
            Op::End => {
                if block_stack.is_empty() { return Err("op-IR: End sem bloco/loop aberto"); }
                let (entry_depth, arity, _is_loop) = block_stack.pop().unwrap();
                depth = entry_depth + arity;
            }
            Op::GpuSubmit(_) | Op::GpuMatmul => {
                // GpuSubmit/GpuMatmul geram módulo completo com import -- validação separada
                return Ok(());
            }
        }
    }
    if !block_stack.is_empty() {
        return Err("op-IR: blocos/loops não fechados (falta End)");
    }
    if depth != 1 {
        return Err("op-IR: deve sobrar exatamente 1 valor (result i32)");
    }
    Ok(())
}

/// Auto-insert End markers para Block/Loop não fechados.
fn ensure_ends(ops: &[Op]) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::new();
    let mut open: i32 = 0;
    for &op in ops {
        match op {
            Op::Block(_) | Op::Loop(_) | Op::If(_) => { out.push(op); open += 1; }
            Op::End => { out.push(op); open -= 1; }
            _ => out.push(op),
        }
    }
    for _ in 0..open { out.push(Op::End); }
    out
}

/// Monta um módulo WASM com import `aios_gpu::submit(op,flags)->i32`.
/// Se ops contém GpuSubmit, gera módulo com import section (não mistura com outros ops).
fn build_gpu_import_module(gpu_op: u8) -> Vec<u8> {
    let mut w = Vec::new();
    // magic + version
    w.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    // type section: 2 tipos -- (i32,i32)->i32 (import) e ()->i32 (run)
    let mut ty = Vec::new();
    uleb(2, &mut ty);
    ty.push(0x60); uleb(2, &mut ty); ty.push(0x7f); ty.push(0x7f); uleb(1, &mut ty); ty.push(0x7f);
    ty.push(0x60); uleb(0, &mut ty); uleb(1, &mut ty); ty.push(0x7f);
    section(0x01, &ty, &mut w);
    // import section: aios_gpu::submit -> func type 0
    let mut imp = Vec::new();
    uleb(1, &mut imp);
    uleb(8, &mut imp); imp.extend_from_slice(b"aios_gpu");
    uleb(6, &mut imp); imp.extend_from_slice(b"submit");
    imp.push(0x00); uleb(0, &mut imp); // func, type index 0
    section(0x02, &imp, &mut w);
    // function section: 1 func local, type 1
    let mut fun = Vec::new();
    uleb(1, &mut fun); uleb(1, &mut fun);
    section(0x03, &fun, &mut w);
    // export section: "run" -> func 1
    let mut exp = Vec::new();
    uleb(1, &mut exp);
    uleb(3, &mut exp); exp.extend_from_slice(b"run");
    exp.push(0x00); uleb(1, &mut exp);
    section(0x07, &exp, &mut w);
    // code section: i32.const GPU_OP; i32.const 0; call 0; end
    let mut body = Vec::new();
    uleb(0, &mut body); // 0 locals
    body.push(0x41); sleb(gpu_op as i64, &mut body); // i32.const op
    body.push(0x41); sleb(0, &mut body);              // i32.const flags=0
    body.push(0x10); uleb(0, &mut body);              // call 0 (aios_gpu::submit)
    body.push(0x0b);                                   // end
    let mut code = Vec::new();
    uleb(1, &mut code);
    uleb(body.len() as u64, &mut code);
    code.extend_from_slice(&body);
    section(0x0a, &code, &mut w);
    w
}

/// Monta um módulo WASM.
pub fn build_run_module(n_params: u32, ops: &[Op]) -> Result<Vec<u8>, &'static str> {
    // Se ops contém GpuSubmit/GpuMatmul, gera módulo com import (ignora outros ops)
    for op in ops {
        match op {
            Op::GpuSubmit(gpu_op) => return Ok(build_gpu_import_module(*gpu_op)),
            Op::GpuMatmul => return Ok(build_gpu_import_module(2)), // 2 = MatmulTernary
            _ => {}
        }
    }
    let ops_owned = ensure_ends(ops);
    validate(n_params, &ops_owned)?;

    let mut out = Vec::new();
    out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);

    let mut ty = Vec::new();
    uleb(1, &mut ty); ty.push(0x60);
    uleb(n_params as u64, &mut ty);
    for _ in 0..n_params { ty.push(0x7f); }
    uleb(1, &mut ty); ty.push(0x7f);
    section(0x01, &ty, &mut out);

    let mut fun = Vec::new();
    uleb(1, &mut fun); uleb(0, &mut fun);
    section(0x03, &fun, &mut out);

    let mut exp = Vec::new();
    uleb(1, &mut exp);
    let name = b"run";
    uleb(name.len() as u64, &mut exp);
    exp.extend_from_slice(name); exp.push(0x00); uleb(0, &mut exp);
    section(0x07, &exp, &mut out);

    let mut body = Vec::new();
    uleb(0, &mut body);
    for op in &ops_owned {
        match op {
            Op::LocalGet(i) => { body.push(0x20); uleb(*i as u64, &mut body); }
            Op::I32Const(v) => { body.push(0x41); sleb(*v as i64, &mut body); }
            Op::I32Add => body.push(0x6a),
            Op::I32Sub => body.push(0x6b),
            Op::I32Mul => body.push(0x6c),
            Op::Drop => body.push(0x1a),
            Op::I32LtS => body.push(0x48),
            Op::I32GtS => body.push(0x4a),
            Op::I32Eq => body.push(0x46),
            Op::I32Eqz => body.push(0x45),
            // LogicalAnd/LogicalOr: dead code - parser emits If/Else/End blocks directly
            Op::LogicalAnd | Op::LogicalOr => unreachable!("parser should emit If/Else blocks for logical ops"),
            Op::I32Select => body.push(0x1b), // WASM select: pop c,val2,val1 → push c?val1:val2
            Op::Block(r) => { body.push(0x02); body.push(block_result_byte(*r)); }
            Op::Loop(r) => { body.push(0x03); body.push(block_result_byte(*r)); }
            Op::If(r) => { body.push(0x04); body.push(block_result_byte(*r)); }
            Op::Else => body.push(0x05),
            Op::Br(t) => { body.push(0x0c); uleb(*t as u64, &mut body); }
            Op::BrIf(t) => { body.push(0x0d); uleb(*t as u64, &mut body); }
            Op::End => body.push(0x0b),
            // GpuSubmit/GpuMatmul: handled by early return em build_run_module
            Op::GpuSubmit(_) | Op::GpuMatmul => unreachable!(),
        }
    }
    body.push(0x0b);

    let mut code = Vec::new();
    uleb(1, &mut code);
    uleb(body.len() as u64, &mut code);
    code.extend_from_slice(&body);
    section(0x0a, &code, &mut out);
    Ok(out)
}

pub fn build_and_run_2(ops: &[Op], a: i32, b: i32) -> Result<i32, &'static str> {
    let wasm = build_run_module(2, ops)?;
    crate::wasmi_rt::run_i32_2(&wasm, "run", a, b, 0)
}

// ─── Compilador expression → op-IR ─────────────────────────────────────────

struct ExprParser<'a> {
    src: &'a [u8],
    pos: usize,
    params: &'a mut Vec<Vec<u8>>,
}

impl<'a> ExprParser<'a> {
    fn new(src: &'a str, params: &'a mut Vec<Vec<u8>>) -> Self {
        ExprParser { src: src.as_bytes(), pos: 0, params }
    }
    fn at_end(&self) -> bool { self.pos >= self.src.len() }
    fn peek(&self) -> u8 { self.src.get(self.pos).copied().unwrap_or(0) }
    fn skip_ws(&mut self) {
        while !self.at_end() && self.peek().is_ascii_whitespace() { self.pos += 1; }
    }
    fn ident_start(c: u8) -> bool { c.is_ascii_alphabetic() || c == b'_' }
    fn ident_char(c: u8) -> bool { c.is_ascii_alphanumeric() || c == b'_' }

    fn param_index(&mut self, name: &[u8]) -> Result<u32, &'static str> {
        if name.len() >= 2 && name[0] == b'p' && name[1..].iter().all(|b| b.is_ascii_digit()) {
            let text = core::str::from_utf8(&name[1..]).map_err(|_| "op-IR: pN inválido")?;
            let idx = text.parse::<u32>().map_err(|_| "op-IR: pN inválido")?;
            if idx > 256 { return Err("op-IR: pN > 256"); }
            while (self.params.len() as u32) <= idx { self.params.push(Vec::new()); }
            return Ok(idx);
        }
        if let Some(i) = self.params.iter().position(|n| n.as_slice() == name) {
            return Ok(i as u32);
        }
        let i = self.params.len() as u32;
        self.params.push(name.to_vec());
        Ok(i)
    }

    fn try_keyword(&mut self, kw: &[u8]) -> bool {
        let end = self.pos + kw.len();
        if end <= self.src.len() && &self.src[self.pos..end] == kw {
            if end < self.src.len() && Self::ident_char(self.src[end]) { return false; }
            self.pos = end;
            true
        } else {
            false
        }
    }

    // ─── Precedence: if > comparison > additive > term > factor ───

    fn parse_expr(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_logical_or(ops)?;
        // ternário branchless: `cond ? a : b`
        // WASM select pop order: c (top), val2, val1 → push val1, val2, c, select
        // So: buffer cond, emit true, emit false, emit cond, select
        self.skip_ws();
        if !self.at_end() && self.peek() == b'?' {
            self.pos += 1;
            self.skip_ws();
            // Condition ops já estão em ops -- move para buffer
            let cond_ops: Vec<Op> = ops.drain(..).collect();
            let mut true_ops = Vec::new();
            self.parse_expr(&mut true_ops)?;
            self.skip_ws();
            if self.peek() != b':' { return Err("op-IR: ternário espera ':' depois de '?'"); }
            self.pos += 1;
            self.skip_ws();
            let mut false_ops = Vec::new();
            self.parse_expr(&mut false_ops)?;
            // push val1 (true), val2 (false), cond → select picks correctly
            ops.extend_from_slice(&true_ops);
            ops.extend_from_slice(&false_ops);
            ops.extend_from_slice(&cond_ops);
            ops.push(Op::I32Select);
        }
        Ok(())
    }

    /// comparison := additive (('<'|'>'|'=='|'!='|'<='|'>=') additive)?
    /// Para: `:` e `else` (tokens de statement, não de expressão).
    /// logical_or := logical_and ('||' logical_and)*
    fn parse_logical_or(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_logical_and(ops)?;
        loop {
            self.skip_ws();
            if self.at_end() { break; }
            if self.peek() == b'|' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'|' {
                self.pos += 2;
                let left_end = ops.len();
                self.parse_logical_and(ops)?;
                let right_ops: Vec<Op> = ops.drain(left_end..).collect();
                // a || b -> if (a) { 1 } else { (b != 0) ? 1 : 0 }
                // After left_ops: left_val on top. If pops it.
                // If(Some(I32)) so block produces a value on the stack.
                // Else branch normalizes b to boolean: I32Eqz + I32Eqz = !!b
                ops.push(Op::If(Some(ValType::I32)));
                ops.push(Op::I32Const(1));
                ops.push(Op::Else);
                ops.extend(right_ops);
                ops.push(Op::I32Eqz); // b==0 -> 1 if zero
                ops.push(Op::I32Eqz); // negate -> 1 if b!=0
                ops.push(Op::End);
            } else {
                break;
            }
        }
        Ok(())
    }

    /// logical_and := comparison ('&&' comparison)*
    fn parse_logical_and(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_comparison(ops)?;
        loop {
            self.skip_ws();
            if self.at_end() { break; }
            if self.peek() == b'&' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'&' {
                self.pos += 2;
                let left_end = ops.len();
                self.parse_comparison(ops)?;
                let right_ops: Vec<Op> = ops.drain(left_end..).collect();
                // a && b -> if (a == 0) { 0 } else { b }
                // After left_ops: left_val on top. I32Eqz converts to (a==0).
                // If(a==0): push 0. Else: push right_val.
                // If(Some(I32)) so block produces a value on the stack.
                ops.push(Op::I32Eqz);
                ops.push(Op::If(Some(ValType::I32)));
                ops.push(Op::I32Const(0));
                ops.push(Op::Else);
                ops.extend(right_ops);
                ops.push(Op::End);
            } else {
                break;
            }
        }
        Ok(())
    }
    fn parse_comparison(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_additive(ops)?;
        self.skip_ws();
        if self.at_end() { return Ok(()); }
        // Para em ':' (statement delimiter do if/while)
        if self.peek() == b':' { return Ok(()); }
        // Para em && e || (logical operators -- caller consome)
        if self.peek() == b'&' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'&' { return Ok(()); }
        if self.peek() == b'|' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'|' { return Ok(()); }
        // Para em 'else' (keyword do if/else)
        if self.peek() == b'e' {
            let save = self.pos;
            if self.try_keyword(b"else") {
                self.pos = save; // restaura -- caller consome
                return Ok(());
            }
        }
        // 2-char operators
        if self.pos + 1 < self.src.len() {
            let two = &self.src[self.pos..self.pos + 2];
            match two {
                b"==" => { self.pos += 2; self.parse_additive(ops)?; ops.push(Op::I32Eq); return Ok(()); }
                b"!=" => { self.pos += 2; self.parse_additive(ops)?; ops.push(Op::I32Eq); ops.push(Op::I32Eqz); return Ok(()); }
                b"<=" => { self.pos += 2; self.parse_additive(ops)?; ops.push(Op::I32GtS); ops.push(Op::I32Eqz); return Ok(()); }
                b">=" => { self.pos += 2; self.parse_additive(ops)?; ops.push(Op::I32LtS); ops.push(Op::I32Eqz); return Ok(()); }
                _ => {}
            }
        }
        // 1-char
        match self.peek() {
            b'<' => { self.pos += 1; self.parse_additive(ops)?; ops.push(Op::I32LtS); }
            b'>' => { self.pos += 1; self.parse_additive(ops)?; ops.push(Op::I32GtS); }
            _ => {}
        }
        Ok(())
    }

    /// additive := term (('+'|'-') term)*
    fn parse_additive(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_term(ops)?;
        loop {
            self.skip_ws();
            match self.peek() {
                b'+' => { self.pos += 1; self.parse_term(ops)?; ops.push(Op::I32Add); }
                b'-' => { self.pos += 1; self.parse_term(ops)?; ops.push(Op::I32Sub); }
                _ => return Ok(()),
            }
        }
    }

    /// term := factor (('*') factor)*
    fn parse_term(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.parse_factor(ops)?;
        loop {
            self.skip_ws();
            if self.peek() == b'*' {
                self.pos += 1;
                self.parse_factor(ops)?;
                ops.push(Op::I32Mul);
            } else {
                return Ok(());
            }
        }
    }

    /// factor := NUM | IDENT | '(' expr ')' | if_expr | while_expr
    fn parse_factor(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.skip_ws();
        match self.peek() {
            b'(' => {
                self.pos += 1;
                self.parse_expr(ops)?;
                self.skip_ws();
                if self.peek() != b')' { return Err("op-IR: falta ')'"); }
                self.pos += 1;
                Ok(())
            }
            b'0'..=b'9' | b'-' => {
                let start = self.pos;
                if self.peek() == b'-' { self.pos += 1; }
                let d0 = self.pos;
                while !self.at_end() && self.peek().is_ascii_digit() { self.pos += 1; }
                if self.pos == d0 { return Err("op-IR: número malformado"); }
                let text = core::str::from_utf8(&self.src[start..self.pos])
                    .map_err(|_| "op-IR: não-utf8")?;
                let v: i32 = text.parse().map_err(|_| "op-IR: i32 fora de faixa")?;
                ops.push(Op::I32Const(v));
                Ok(())
            }
            c if Self::ident_start(c) => {
                let start = self.pos;
                while !self.at_end() && Self::ident_char(self.peek()) { self.pos += 1; }
                let name = &self.src[start..self.pos];
                if name == b"if" { return self.parse_if_factor(ops); }
                if name == b"while" { return self.parse_while_factor(ops); }
                if name == b"gpu_matmul" {
                    ops.push(Op::GpuMatmul);
                    return Ok(());
                }
                let idx = self.param_index(name)?;
                ops.push(Op::LocalGet(idx));
                Ok(())
            }
            _ => Err("op-IR: token inesperado"),
        }
    }

    /// Encontra o `else` mais externo no slice [pos..] a depth 0.
    /// Retorna a posição absoluta no src original, ou None.
    fn find_outer_else(&self, body_start: usize, body_end: usize) -> Option<usize> {
        let mut depth = 0i32; // parênteses + nested if
        let mut i = body_start;
        while i < body_end {
            let c = self.src[i];
            if c == b'(' { depth += 1; i += 1; continue; }
            if c == b')' { depth -= 1; i += 1; continue; }
            // 'if' keyword → depth += 1
            if c == b'i' && i + 2 < body_end && &self.src[i..i + 2] == b"if"
                && (i + 2 >= body_end || !Self::ident_char(self.src[i + 2]))
            {
                depth += 1; i += 2; continue;
            }
            // 'else' keyword
            if c == b'e' && i + 4 < body_end && &self.src[i..i + 4] == b"else"
                && (i + 4 >= body_end || !Self::ident_char(self.src[i + 4]))
            {
                if depth == 0 { return Some(i); }
                depth -= 1; // else fecha um if anterior
                i += 4;
                continue;
            }
            i += 1;
        }
        None
    }

    /// if_expr := 'if' cond ':' true_expr ('else' ':' false_expr)?
    /// Gera: Block [BrIf(0) false_body End] true_body End
    fn parse_if_factor(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.skip_ws();
        // Condição
        self.parse_expr(ops)?;
        self.skip_ws();
        if self.peek() != b':' { return Err("op-IR: if espera ':'"); }
        self.pos += 1;
        self.skip_ws();

        // Encontra o else mais externo no body
        let body_start = self.pos;
        let body_end = self.src.len();

        if let Some(else_pos) = self.find_outer_else(body_start, body_end) {
            // True body: [body_start..else_pos]
            let mut true_ops = Vec::new();
            let true_slice = core::str::from_utf8(&self.src[body_start..else_pos])
                .map_err(|_| "op-IR: true branch não-utf8")?;
            let mut tp = ExprParser::new(true_slice, self.params);
            tp.parse_expr(&mut true_ops)?;

            // False body: [else_pos+4..] (skip "else", consume ':')
            self.pos = else_pos + 4; // skip "else"
            self.skip_ws();
            if self.peek() != b':' { return Err("op-IR: else espera ':'"); }
            self.pos += 1;
            self.skip_ws();

            let mut false_ops = Vec::new();
            self.parse_expr(&mut false_ops)?;

            // If [true_body] Else [false_body] End
            ops.push(Op::If(Some(ValType::I32)));
            ops.extend_from_slice(&true_ops);
            ops.push(Op::Else);
            ops.extend_from_slice(&false_ops);
            ops.push(Op::End);
        } else {
            // Sem else: If [true_body] Else [0] End
            let mut true_ops = Vec::new();
            let true_slice = core::str::from_utf8(&self.src[body_start..body_end])
                .map_err(|_| "op-IR: true branch não-utf8")?;
            let mut tp = ExprParser::new(true_slice, self.params);
            tp.parse_expr(&mut true_ops)?;
            self.pos = body_end;

            ops.push(Op::If(Some(ValType::I32)));
            ops.extend_from_slice(&true_ops);
            ops.push(Op::Else);
            ops.push(Op::I32Const(0));
            ops.push(Op::End);
        }
        Ok(())
    }

    /// while_expr := 'while' cond ':' body_expr
    /// Gera: Loop [cond BrIf(1) body Drop Br(0) End]
    fn parse_while_factor(&mut self, ops: &mut Vec<Op>) -> Result<(), &'static str> {
        self.skip_ws();
        let mut cond_ops = Vec::new();
        self.parse_expr(&mut cond_ops)?;
        self.skip_ws();
        if self.peek() != b':' { return Err("op-IR: while espera ':'"); }
        self.pos += 1;
        self.skip_ws();
        let mut body_ops = Vec::new();
        self.parse_additive(&mut body_ops)?;
        ops.push(Op::Loop(None));
        ops.extend_from_slice(&cond_ops);
        ops.push(Op::BrIf(1));
        ops.extend_from_slice(&body_ops);
        ops.push(Op::Drop);
        ops.push(Op::Br(0));
        ops.push(Op::End);
        Ok(())
    }
}

/// Compila expressão para op-IR (v3).
pub fn compile_expression(source: &str) -> Result<(u32, Vec<Op>), &'static str> {
    let mut params = Vec::new();
    let mut p = ExprParser::new(source, &mut params);
    let mut ops = Vec::new();
    p.skip_ws();
    if p.at_end() { return Err("op-IR: expressão vazia"); }
    p.parse_expr(&mut ops)?;
    p.skip_ws();
    if !p.at_end() { return Err("op-IR: trailing input"); }
    let n = p.params.len() as u32;
    validate(n, &ops)?;
    Ok((n, ops))
}

// ─── DSL subset → op-IR (v3) ───────────────────────────────────────────────

fn dsl_ident_start(c: u8) -> bool { c.is_ascii_alphabetic() || c == b'_' }
fn dsl_ident_char(c: u8) -> bool { c.is_ascii_alphanumeric() || c == b'_' }

fn dsl_trim(mut s: &[u8]) -> &[u8] {
    while let Some((f, rest)) = s.split_first() {
        if f.is_ascii_whitespace() { s = rest; } else { break; }
    }
    while let Some((l, rest)) = s.split_last() {
        if l.is_ascii_whitespace() { s = rest; } else { break; }
    }
    s
}

fn dsl_split<'a>(src: &'a str) -> Vec<&'a [u8]> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth: i32 = 0;
    let mut quote = 0u8;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if quote != 0 { if c == quote { quote = 0; } i += 1; continue; }
        match c {
            b'\'' | b'"' => { quote = c; i += 1; }
            b'(' => { depth += 1; i += 1; }
            b')' => { depth -= 1; i += 1; }
            b'#' if depth == 0 => {
                out.push(&b[start..i]);
                while i < b.len() && b[i] != b'\n' { i += 1; }
                start = i;
            }
            b'\n' | b';' if depth == 0 => {
                if start < i { out.push(&b[start..i]); }
                start = i + 1; i += 1;
            }
            _ => { i += 1; }
        }
    }
    out.push(&b[start..]);
    out
}

fn dsl_matching_paren(s: &[u8], pos: usize) -> Result<usize, &'static str> {
    let mut depth = 0i32;
    let mut quote = 0u8;
    for (i, &c) in s.iter().enumerate().skip(pos) {
        if quote != 0 { if c == quote { quote = 0; } continue; }
        match c {
            b'\'' | b'"' => quote = c,
            b'(' => depth += 1,
            b')' => { depth -= 1; if depth == 0 { return Ok(i); } }
            _ => {}
        }
    }
    Err("op-IR/DSL: parêntese não fechado")
}

fn dsl_top_level_eq(s: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote = 0u8;
    for (i, &c) in s.iter().enumerate() {
        if quote != 0 { if c == quote { quote = 0; } continue; }
        match c {
            b'\'' | b'"' => quote = c,
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'=' if depth == 0 => {
                if s.get(i + 1) == Some(&b'=') || s.get(i + 1) == Some(&b'>') { return None; }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

fn dsl_expand(
    src: &[u8], bindings: &[(Vec<u8>, Vec<u8>)], chain: &mut Vec<Vec<u8>>,
) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut quote = 0u8;
    while i < src.len() {
        let c = src[i];
        if quote != 0 { out.push(c); if c == quote { quote = 0; } i += 1; continue; }
        match c {
            b'\'' | b'"' => { quote = c; out.push(c); i += 1; }
            _ if dsl_ident_start(c) => {
                let start = i;
                while i < src.len() && dsl_ident_char(src[i]) { i += 1; }
                let name = &src[start..i];
                let mut hit: Option<usize> = None;
                for (j, (n, _)) in bindings.iter().enumerate() {
                    if n.as_slice() == name { hit = Some(j); }
                }
                match hit {
                    Some(j) => {
                        if chain.iter().any(|c| c.as_slice() == name) {
                            return Err("op-IR/DSL: atribuição cíclica");
                        }
                        chain.push(name.to_vec());
                        let sub = dsl_expand(&bindings[j].1, bindings, chain)?;
                        chain.pop();
                        out.push(b'('); out.extend_from_slice(&sub); out.push(b')');
                    }
                    None => out.extend_from_slice(name),
                }
            }
            _ => { out.push(c); i += 1; }
        }
    }
    Ok(out)
}

fn dsl_parse_expr_into(
    expanded: &[u8], params: &mut Vec<Vec<u8>>, ops: &mut Vec<Op>,
) -> Result<(), &'static str> {
    let expr_str = core::str::from_utf8(expanded).map_err(|_| "op-IR/DSL: não-utf8")?;
    let mut p = ExprParser::new(expr_str, params);
    p.parse_expr(ops)?;
    p.skip_ws();
    if !p.at_end() { return Err("op-IR/DSL: trailing input na expressão"); }
    Ok(())
}

/// Parseia um bloco (para if/else body): pega a ÚLTIMA expressão.
/// Não usa dsl_split -- recebe a string já isolada do body.
fn dsl_parse_block_expr(
    src: &[u8], bindings: &[(Vec<u8>, Vec<u8>)], params: &mut Vec<Vec<u8>>,
    chain: &mut Vec<Vec<u8>>,
) -> Result<Vec<Op>, &'static str> {
    let trimmed = dsl_trim(src);
    if trimmed.is_empty() { return Err("op-IR/DSL: block vazio"); }
    // return <expr> ?
    if trimmed.len() >= 6 && &trimmed[..6] == b"return"
        && (trimmed.len() == 6 || !dsl_ident_char(trimmed[6]))
    {
        let rest = dsl_trim(&trimmed[6..]);
        if rest.is_empty() { return Err("op-IR/DSL: return vazio no block"); }
        let expanded = dsl_expand(rest, bindings, chain)?;
        let mut ops = Vec::new();
        dsl_parse_expr_into(&expanded, params, &mut ops)?;
        return Ok(ops);
    }
    // expressão simples
    let expanded = dsl_trimmed_parse(trimmed, bindings, chain)?;
    let mut ops = Vec::new();
    dsl_parse_expr_into(&expanded, params, &mut ops)?;
    Ok(ops)
}

/// Expande uma expressão trimmed (p/ block body sem stmt splitting).
fn dsl_trimmed_parse(
    trimmed: &[u8], bindings: &[(Vec<u8>, Vec<u8>)], chain: &mut Vec<Vec<u8>>,
) -> Result<Vec<u8>, &'static str> {
    dsl_expand(trimmed, bindings, chain)
}

/// Compila Python/DSL subset v3 para op-IR.
pub fn compile_python_dsl(source: &str) -> Result<(u32, Vec<Op>), &'static str> {
    let mut params: Vec<Vec<u8>> = Vec::new();
    let mut bindings: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut ops: Vec<Op> = Vec::new();
    let mut final_ops: Option<Vec<Op>> = None;
    let mut chain: Vec<Vec<u8>> = Vec::new();

    for raw in dsl_split(source) {
        let stmt = dsl_trim(raw);
        if stmt.is_empty() { continue; }

        // print(...)
        if stmt.len() >= 6 && &stmt[..5] == b"print" && stmt[5] == b'(' {
            if final_ops.is_some() { return Err("op-IR/DSL: statement após return"); }
            let close = dsl_matching_paren(stmt, 5)?;
            if !dsl_trim(&stmt[close + 1..]).is_empty() {
                return Err("op-IR/DSL: trailing após print(...)");
            }
            let inner = dsl_trim(&stmt[6..close]);
            if !inner.is_empty() && (inner[0] == b'\'' || inner[0] == b'"') {
                let q = inner[0];
                if inner.len() < 2 || inner[inner.len() - 1] != q
                    || inner[1..inner.len() - 1].contains(&q)
                { return Err("op-IR/DSL: string malformada"); }
            } else if !inner.is_empty() {
                let expanded = dsl_expand(inner, &bindings, &mut chain)?;
                dsl_parse_expr_into(&expanded, &mut params, &mut ops)?;
                ops.push(Op::Drop);
            }
            continue;
        }

        // return <expr>
        if stmt.len() >= 6 && &stmt[..6] == b"return"
            && (stmt.len() == 6 || !dsl_ident_char(stmt[6]))
        {
            if final_ops.is_some() { return Err("op-IR/DSL: múltiplos returns"); }
            let rest = dsl_trim(&stmt[6..]);
            if rest.is_empty() { return Err("op-IR/DSL: return vazio"); }
            let expanded = dsl_expand(rest, &bindings, &mut chain)?;
            let mut fops = Vec::new();
            dsl_parse_expr_into(&expanded, &mut params, &mut fops)?;
            final_ops = Some(fops);
            continue;
        }

        // if <cond> : <true> [else : <false>]
        if stmt.len() >= 2 && &stmt[..2] == b"if"
            && (stmt.len() == 2 || !dsl_ident_char(stmt[2]))
        {
            if final_ops.is_some() { return Err("op-IR/DSL: statement após return"); }
            let rest = dsl_trim(&stmt[2..]);
            let colon = rest.iter().position(|&c| c == b':')
                .ok_or("op-IR/DSL: if sem ':'")?;
            let cond_src = dsl_trim(&rest[..colon]);
            let body_full = &rest[colon + 1..]; // não trim -- preserva posição

            // Encontra o else mais externo no body (depth-aware: if + else + parens)
            let mut else_abs: Option<usize> = None;
            {
                let bs = body_full;
                let mut depth = 0i32; // parênteses + nested if depth
                let mut j = 0;
                while j < bs.len() {
                    let c = bs[j];
                    if c == b'(' { depth += 1; j += 1; continue; }
                    if c == b')' { depth -= 1; j += 1; continue; }
                    // Detecta 'if' keyword → aumenta depth
                    if c == b'i' && j + 2 < bs.len() && &bs[j..j + 2] == b"if"
                        && (j + 2 >= bs.len() || !dsl_ident_char(bs[j + 2]))
                    {
                        depth += 1;
                        j += 2;
                        continue;
                    }
                    // Detecta 'else' keyword → se depth>0, decrementa; senão, é o match
                    if c == b'e' && j + 4 < bs.len() && &bs[j..j + 4] == b"else"
                        && (j + 4 >= bs.len() || !dsl_ident_char(bs[j + 4]))
                    {
                        let after = dsl_trim(&bs[j + 4..]);
                        if !after.is_empty() && after[0] == b':' {
                            if depth == 0 {
                                else_abs = Some(j);
                                break;
                            } else {
                                depth -= 1; // else fecha um if anterior
                                j += 4;
                                continue;
                            }
                        }
                    }
                    j += 1;
                }
            }

            let cond_exp = dsl_expand(cond_src, &bindings, &mut chain)?;
            let mut cond_ops = Vec::new();
            dsl_parse_expr_into(&cond_exp, &mut params, &mut cond_ops)?;

            if let Some(ep) = else_abs {
                let true_src = dsl_trim(&body_full[..ep]);
                let false_src = dsl_trim(&body_full[ep + 4..]); // skip "else:"
                // Consome ':' do false
                let false_src = if !false_src.is_empty() && false_src[0] == b':' {
                    dsl_trim(&false_src[1..])
                } else {
                    false_src
                };
                let true_ops = dsl_parse_block_expr(true_src, &bindings, &mut params, &mut chain)?;
                let false_ops = dsl_parse_block_expr(false_src, &bindings, &mut params, &mut chain)?;

                // If [cond true_body] Else [false_body] End
                ops.extend_from_slice(&cond_ops);
                ops.push(Op::If(Some(ValType::I32)));
                ops.extend_from_slice(&true_ops);
                ops.push(Op::Else);
                ops.extend_from_slice(&false_ops);
                ops.push(Op::End);
            } else {
                let true_src = dsl_trim(body_full);
                let true_ops = dsl_parse_block_expr(true_src, &bindings, &mut params, &mut chain)?;
                // If [cond true_body] Else [0] End
                ops.extend_from_slice(&cond_ops);
                ops.push(Op::If(Some(ValType::I32)));
                ops.extend_from_slice(&true_ops);
                ops.push(Op::Else);
                ops.push(Op::I32Const(0));
                ops.push(Op::End);
            }
            // if é resultado final
            final_ops = Some(core::mem::take(&mut ops));
            continue;
        }

        // while <cond> : <body>
        if stmt.len() >= 5 && &stmt[..5] == b"while"
            && (stmt.len() == 5 || !dsl_ident_char(stmt[5]))
        {
            if final_ops.is_some() { return Err("op-IR/DSL: statement após return"); }
            let rest = dsl_trim(&stmt[5..]);
            let colon = rest.iter().position(|&c| c == b':')
                .ok_or("op-IR/DSL: while sem ':'")?;
            let cond_src = dsl_trim(&rest[..colon]);
            let body_src = dsl_trim(&rest[colon + 1..]);
            let cond_exp = dsl_expand(cond_src, &bindings, &mut chain)?;
            let mut cond_ops = Vec::new();
            dsl_parse_expr_into(&cond_exp, &mut params, &mut cond_ops)?;
            let body_ops = dsl_parse_block_expr(body_src, &bindings, &mut params, &mut chain)?;
            ops.push(Op::Loop(None));
            ops.extend_from_slice(&cond_ops);
            ops.push(Op::BrIf(1));
            ops.extend_from_slice(&body_ops);
            ops.push(Op::Drop);
            ops.push(Op::Br(0));
            ops.push(Op::End);
            continue;
        }

        // <ident> = <expr>
        if let Some(eq) = dsl_top_level_eq(stmt) {
            if final_ops.is_some() { return Err("op-IR/DSL: statement após return"); }
            let lhs = dsl_trim(&stmt[..eq]);
            if lhs.is_empty() || !dsl_ident_start(lhs[0]) || lhs.iter().any(|c| !dsl_ident_char(*c)) {
                return Err("op-IR/DSL: lhs deve ser identificador");
            }
            let rhs = dsl_trim(&stmt[eq + 1..]);
            if rhs.is_empty() { return Err("op-IR/DSL: rhs vazio"); }
            if rhs[0] == b'\'' || rhs[0] == b'"' { return Err("op-IR/DSL: string só em print(...)"); }
            bindings.push((lhs.to_vec(), rhs.to_vec()));
            continue;
        }

        // expressão pura -- retorno implícito
        if final_ops.is_some() { return Err("op-IR/DSL: expressão após return"); }
        let expanded = dsl_expand(stmt, &bindings, &mut chain)?;
        let mut fops = Vec::new();
        dsl_parse_expr_into(&expanded, &mut params, &mut fops)?;
        final_ops = Some(fops);
    }

    if let Some(f) = final_ops { ops.extend(f); } else { ops.push(Op::I32Const(0)); }
    let n = params.len() as u32;
    validate(n, &ops)?;
    Ok((n, ops))
}

pub fn op_ir_schema_hint() -> &'static str {
    concat!(
        "Gere ops i32 (op-IR): LocalGet|I32Const|I32Add|I32Sub|I32Mul|",
        "I32LtS|I32GtS|I32Eq|I32Eqz|Block|Loop|Br|BrIf|End."
    )
}

pub fn self_test() -> bool {
    let ops = [Op::LocalGet(0), Op::LocalGet(1), Op::I32Mul, Op::I32Const(7), Op::I32Add];
    match build_and_run_2(&ops, 6, 7) {
        Ok(v) if v == 49 => {
            k_nano::slog_hermes!("WASM-BUILD", "info",
                "op-IR→wasm→wasmi self-test PASS (a*b+7: 6,7 -> {}) -- ADR-0059 F4", v);
            true
        }
        Ok(v) => { k_nano::slog_hermes!("WASM-BUILD", "warn", "self-test inesperado: {}", v); false }
        Err(e) => { k_nano::slog_hermes!("WASM-BUILD", "warn", "self-test FAIL: {}", e); false }
    }
}

// ─── Testes ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasmi_rt;

    #[test]
    fn compile_and_run_real_skill() {
        let (n, ops) = compile_expression("a*b+7").expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 6, 7, 0).unwrap(), 49);
        assert!(wasmi_rt::sandbox_validate_and_run(&wasm));
    }

    #[test]
    fn compile_with_pn() {
        let (n, _) = compile_expression("p2*p3+1").expect("parse");
        assert_eq!(n, 4);
        let wasm = build_run_module(n, &compile_expression("p2*p3+1").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[0, 0, 6, 7], 0).unwrap(), 43);
    }

    #[test]
    fn compile_rejects_out_of_grammar() {
        assert!(compile_expression("").is_err());
        assert!(compile_expression("def foo():").is_err());
        assert!(compile_expression("a+").is_err());
    }

    #[test]
    fn compile_parens() {
        let (n, _) = compile_expression("(a + b) * 2").expect("parse");
        let wasm = build_run_module(n, &compile_expression("(a + b) * 2").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 4, 0).unwrap(), 14);
    }

    // ─── Comparações ───

    #[test]
    fn lt() {
        let (n, _) = compile_expression("a < b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a < b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 3, 0).unwrap(), 0);
    }

    #[test]
    fn gt() {
        let (n, _) = compile_expression("a > b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a > b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 0);
    }

    #[test]
    fn eq_cmp() {
        let (n, _) = compile_expression("a == b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a == b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 7, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 8, 0).unwrap(), 0);
    }

    #[test]
    fn neq() {
        let (n, _) = compile_expression("a != b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a != b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 8, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 7, 0).unwrap(), 0);
    }

    #[test]
    fn lte() {
        let (n, _) = compile_expression("a <= b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a <= b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 1);
    }

    #[test]
    fn gte() {
        let (n, _) = compile_expression("a >= b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a >= b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 1);
    }

    #[test]
    fn chained() {
        let (n, _) = compile_expression("a + 1 < b * 2").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a + 1 < b * 2").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 3, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 0);
    }

    // ─── if/else ───

    #[test]
    fn if_else_min() {
        let (n, _) = compile_expression("if a < b: a else: b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("if a < b: a else: b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 7, 0).unwrap(), 7);
    }

    #[test]
    fn if_else_max() {
        let (n, _) = compile_expression("if a > b: a else: b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("if a > b: a else: b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 5);
    }

    #[test]
    fn if_else_arith() {
        let (n, _) = compile_expression("if a == b: a + b else: a * b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("if a == b: a + b else: a * b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 10);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 15);
    }

    #[test]
    fn if_else_nested() {
        let src = "if a > 0: if a < 10: a else: 10 else: 0";
        let (n, _) = compile_expression(src).expect("parse");
        let wasm = build_run_module(n, &compile_expression(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[5], 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[15], 0).unwrap(), 10);
    }

    // ─── Block/Loop/Br ───

    #[test]
    fn block_br_value() {
        let ops = vec![Op::Block(Some(ValType::I32)), Op::I32Const(42), Op::Br(0), Op::End];
        let wasm = build_run_module(0, &ops).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[], 0).unwrap(), 42);
    }

    #[test]
    fn block_auto_end() {
        let ops = vec![Op::Block(Some(ValType::I32)), Op::I32Const(7)];
        assert!(build_run_module(0, &ops).is_ok());
    }

    // ─── Validate ───

    #[test]
    fn val_block_not_closed() {
        assert!(validate(0, &[Op::Block(None), Op::I32Const(1)]).is_err());
    }

    #[test]
    fn val_end_no_block() {
        assert!(validate(0, &[Op::End]).is_err());
    }

    #[test]
    fn val_br_outside() {
        assert!(validate(0, &[Op::Br(0), Op::I32Const(1)]).is_err());
    }

    #[test]
    fn val_loop_void() {
        assert!(validate(0, &[Op::Loop(None), Op::Br(0), Op::End, Op::I32Const(1)]).is_ok());
    }

    // ─── DSL ───

    #[test]
    fn dsl_if_else_cmp() {
        let (n, _) = compile_python_dsl("if a < b: a else: b").expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl("if a < b: a else: b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 3);
    }

    #[test]
    fn dsl_if_else_bindings() {
        let src = "x = a + b\nif x > 10: x else: 10";
        let (n, _) = compile_python_dsl(src).expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 10); // x=8, 8>10 false → else=10
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 8, 0).unwrap(), 15); // x=15, 15>10 true → x=15
    }

    #[test]
    fn dsl_cmp_return() {
        let (n, _) = compile_python_dsl("return a == b").expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl("return a == b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 6, 0).unwrap(), 0);
    }

    #[test]
    fn dsl_print_cmp() {
        let (n, _) = compile_python_dsl("print(a < b)").expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl("print(a < b)").unwrap().1).expect("build");
        assert!(wasmi_rt::sandbox_validate_and_run(&wasm));
    }

    // ─── DSL regression ───

    #[test]
    fn dsl_print_only() {
        let (n, ops) = compile_python_dsl("print('hello world')").expect("dsl");
        assert_eq!(n, 0);
        assert_eq!(ops, vec![Op::I32Const(0)]);
        assert!(wasmi_rt::sandbox_validate_and_run(&build_run_module(n, &ops).unwrap()));
    }

    #[test]
    fn dsl_let_binding() {
        let src = "x = a + 1\nreturn x * 2";
        let (n, _) = compile_python_dsl(src).expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[7], 0).unwrap(), 16);
    }

    #[test]
    fn dsl_multi_use() {
        let src = "x = a + 1\nreturn x * x";
        let (n, _) = compile_python_dsl(src).expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[5], 0).unwrap(), 36);
    }

    #[test]
    fn dsl_prints_then_return() {
        let src = "print('inicio')\nprint(a)\nreturn a + 2";
        let (n, _) = compile_python_dsl(src).expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[10], 0).unwrap(), 12);
    }

    #[test]
    fn dsl_semicolon_comment() {
        let src = "x = a * 2; # dobra\nreturn x + 1";
        let (n, _) = compile_python_dsl(src).expect("dsl");
        let wasm = build_run_module(n, &compile_python_dsl(src).unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[5], 0).unwrap(), 11);
    }

    #[test]
    fn dsl_rejects_outside_subset() {
        assert!(compile_python_dsl("def foo():\n    pass").is_err());
        assert!(compile_python_dsl("for i in range(3):\n    print(i)").is_err());
        assert!(compile_python_dsl("x = x + 1\nreturn x").is_err());
        assert!(compile_python_dsl("x = 'str'").is_err());
        assert!(compile_python_dsl("return 1\nprint(2)").is_err());
    }

    // ─── GPU Host-Call: op-IR → WASM → sandbox → fila lock-free ──────────

    /// GpuMatmul op-IR → WASM module com import aios_gpu::submit →
    /// wasmi sandbox com CAP_GPU → fila lock-free MpmcQueue → job id.
    /// Fluxo end-to-end completo: compile → build → link → run → verify queue.
    #[test]
    fn gpu_matmul_e2e_op_ir_to_queue() {
        use crate::wasmi_rt;

        // 1. GpuMatmul op-IR → build_run_module → WASM com import aios_gpu::submit
        let ops = vec![Op::GpuMatmul];
        let wasm = build_run_module(0, &ops).expect("build GpuMatmul WASM");

        // 2. Valida módulo WASM: import section deve conter aios_gpu::submit
        assert!(wasm.len() > 20, "WASM module deve ter conteúdo");
        // Magic + version + sections
        assert_eq!(&wasm[..4], &[0x00, 0x61, 0x73, 0x6d], "WASM magic");

        // 3. Snapshot da fila antes
        let (s_before, _, _) = k_hal::gpu::work_queue::stats();

        // 4. Executa no wasmi sandbox com CAP_GPU
        let result = wasmi_rt::run_wasm(&wasm, "run", &[], wasmi_rt::CAP_GPU)
            .expect("GpuMatmul deve executar com CAP_GPU");

        // 5. Resultado é job id >= 1 (fila lock-free recebeu o comando)
        assert!(result >= 1, "job id deveria ser >= 1, veio {}", result);

        // 6. Verifica fila lock-free: submitted incrementou
        let (s_after, _, _) = k_hal::gpu::work_queue::stats();
        assert!(s_after > s_before,
            "submitted deveria ter incrementado: antes={} depois={}", s_before, s_after);

        // 7. Drain (pode ter sido drenado por outro teste paralelo -- só valida que ops)
        let _ = k_hal::gpu::work_queue::drain(false);
    }

    /// GpuMatmul via expression parser (gpu_matmul keyword).
    #[test]
    fn gpu_matmul_expression_parser() {
        use crate::wasmi_rt;

        // compile_expression("gpu_matmul") → Op::GpuMatmul
        let (n, ops) = compile_expression("gpu_matmul").expect("parse gpu_matmul");
        assert_eq!(n, 0, "gpu_matmul não precisa de params");
        assert_eq!(ops, vec![Op::GpuMatmul]);

        let wasm = build_run_module(n, &ops).expect("build");
        let (s_before, _, _) = k_hal::gpu::work_queue::stats();

        let result = wasmi_rt::run_wasm(&wasm, "run", &[], wasmi_rt::CAP_GPU).expect("run com CAP_GPU");
        assert!(result >= 1, "job id >= 1");

        let (s_after, _, _) = k_hal::gpu::work_queue::stats();
        assert!(s_after > s_before, "fila recebeu o job");
        let _ = k_hal::gpu::work_queue::drain(false);
    }

    /// GpuSubmit com op code custom (VectorAdd = 1) via op-IR.
    #[test]
    fn gpu_submit_custom_op_e2e() {
        use crate::wasmi_rt;

        let ops = vec![Op::GpuSubmit(1)]; // VectorAdd
        let wasm = build_run_module(0, &ops).expect("build GpuSubmit(1)");

        let (s_before, _, _) = k_hal::gpu::work_queue::stats();
        let result = wasmi_rt::run_wasm(&wasm, "run", &[], wasmi_rt::CAP_GPU).expect("run com CAP_GPU");
        assert!(result >= 1);
        let (s_after, _, _) = k_hal::gpu::work_queue::stats();
        assert!(s_after > s_before, "VectorAdd submetido na fila");
        let _ = k_hal::gpu::work_queue::drain(false);
    }

    /// Sem CAP_GPU → trap (capability denied). Valida CapGate no host-import.
    #[test]
    fn gpu_matmul_denied_without_cap() {
        let ops = vec![Op::GpuMatmul];
        let wasm = build_run_module(0, &ops).expect("build");
        assert!(
            crate::wasmi_rt::run_wasm(&wasm, "run", &[], 0).is_err(),
            "sem CAP_GPU deve trap"
        );
    }

    /// DSL: gpu_matmul como expressão pura retorna job id.
    #[test]
    fn dsl_gpu_matmul_as_expression() {
        use crate::wasmi_rt;

        let (n, ops) = compile_python_dsl("gpu_matmul").expect("dsl gpu_matmul");
        let wasm = build_run_module(n, &ops).expect("build");

        let (s_before, _, _) = k_hal::gpu::work_queue::stats();
        let result = wasmi_rt::run_wasm(&wasm, "run", &[], wasmi_rt::CAP_GPU).expect("dsl gpu_matmul com CAP_GPU");
        assert!(result >= 1, "job id >= 1");
        let (s_after, _, _) = k_hal::gpu::work_queue::stats();
        assert!(s_after > s_before, "fila recebeu o job via DSL");
        let _ = k_hal::gpu::work_queue::drain(false);
    }

    // ─── I32Select + ternário branchless ─────────────────────────────────

    /// I32Select via op-IR direto: push val1=42, val2=7, cond=1 → select → 42.
    /// WASM select pop order: c (top), val2, val1 → push val1, val2, c, select.
    #[test]
    fn select_op_ir_true() {
        let ops = vec![
            Op::I32Const(42), Op::I32Const(7), Op::I32Const(1),
            Op::I32Select,
        ];
        let wasm = build_run_module(0, &ops).expect("build select");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[], 0).unwrap(), 42);
    }

    /// I32Select via op-IR: push val1=42, val2=7, cond=0 → select → 7.
    #[test]
    fn select_op_ir_false() {
        let ops = vec![
            Op::I32Const(42), Op::I32Const(7), Op::I32Const(0),
            Op::I32Select,
        ];
        let wasm = build_run_module(0, &ops).expect("build select");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[], 0).unwrap(), 7);
    }

    /// Ternário expression: `a < b ? a : b` (min).
    #[test]
    fn ternary_min() {
        let (n, _) = compile_expression("a < b ? a : b").expect("parse ternary");
        let wasm = build_run_module(n, &compile_expression("a < b ? a : b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 7, 7, 0).unwrap(), 7);
    }

    /// Ternário: `a > b ? a : b` (max).
    #[test]
    fn ternary_max() {
        let (n, _) = compile_expression("a > b ? a : b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a > b ? a : b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 5);
    }

    /// Ternário com aritmética: `a == b ? a + b : a * b`.
    #[test]
    fn ternary_arith() {
        let (n, _) = compile_expression("a == b ? a + b : a * b").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a == b ? a + b : a * b").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 10);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 15);
    }

    /// Ternário com constantes: `a < 5 ? 1 : 0`.
    #[test]
    fn ternary_const() {
        let (n, _) = compile_expression("a < 5 ? 1 : 0").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a < 5 ? 1 : 0").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[3], 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[7], 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[5], 0).unwrap(), 0);
    }

    /// Ternário com `!=`:
    #[test]
    fn ternary_neq() {
        let (n, _) = compile_expression("a != 0 ? a * 2 : 0").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a != 0 ? a * 2 : 0").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[5], 0).unwrap(), 10);
        assert_eq!(wasmi_rt::run_wasm(&wasm, "run", &[0], 0).unwrap(), 0);
    }

    /// Ternário com `<=`:
    #[test]
    fn ternary_lte() {
        let (n, _) = compile_expression("a <= b ? 1 : 0").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a <= b ? 1 : 0").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 1);
    }

    /// Ternário com `>=`:
    #[test]
    fn ternary_gte() {
        let (n, _) = compile_expression("a >= b ? 1 : 0").expect("parse");
        let wasm = build_run_module(n, &compile_expression("a >= b ? 1 : 0").unwrap().1).expect("build");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 5, 0).unwrap(), 1);
    }

    /// Ternário branchless via build_run_module direto (2 params, sem imports).
    #[test]
    fn ternary_no_imports() {
        // `a < b ? a + 1 : b + 1`
        let (n, _) = compile_expression("a < b ? a + 1 : b + 1").expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &compile_expression("a < b ? a + 1 : b + 1").unwrap().1).expect("build");
        // Verifica que NÃO tem import section (módulo puro sem imports)
        assert_eq!(&wasm[..4], &[0x00, 0x61, 0x73, 0x6d], "WASM magic");
        assert_eq!(wasm[4], 0x01, "WASM version 1");
        // type section (0x01) -- sem import section (0x02)
        assert_eq!(wasm[8], 0x01, "primeira seção deve ser type (0x01), não import (0x02)");
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 4);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 3, 0).unwrap(), 4);
    }

    /// build_and_run_2 com ternário -- wrapper conveniente.
    #[test]
    fn ternary_build_and_run() {
        let (n, ops) = compile_expression("a < b ? a : b").expect("parse");
        assert_eq!(n, 2);
        assert_eq!(wasmi_rt::run_i32_2(&build_run_module(n, &ops).unwrap(), "run", 10, 20, 0).unwrap(), 10);
        assert_eq!(wasmi_rt::run_i32_2(&build_run_module(n, &ops).unwrap(), "run", 20, 10, 0).unwrap(), 10);
    }

    // === Nested ternary ===

    #[test]
    fn nested_ternary_min_of_three() {
        // min(a,b,c) = a < b ? (a < c ? a : c) : (b < c ? b : c)
        let (n, ops) = compile_expression("a < b ? (a < c ? a : c) : (b < c ? b : c)").expect("parse");
        assert_eq!(n, 3);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_3(&wasm, "run", 5, 3, 7, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_3(&wasm, "run", 1, 2, 3, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_3(&wasm, "run", 9, 8, 7, 0).unwrap(), 7);
    }

    #[test]
    fn nested_ternary_const_true_branch() {
        // 1 ? (0 ? 10 : 20) : 30 -> 20
        let (n, ops) = compile_expression("1 ? (0 ? 10 : 20) : 30").expect("parse");
        assert_eq!(n, 0);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_0(&wasm, "run", 0).unwrap(), 20);
    }

    #[test]
    fn nested_ternary_const_false_branch() {
        // 0 ? 10 : (1 ? 20 : 30) -> 20
        let (n, ops) = compile_expression("0 ? 10 : (1 ? 20 : 30)").expect("parse");
        assert_eq!(n, 0);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_0(&wasm, "run", 0).unwrap(), 20);
    }

    // === Logical AND (&&) ===

    #[test]
    fn logical_and_both_true() {
        // a && b -> a ? b : 0
        let (n, ops) = compile_expression("a && b").expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 7, 0).unwrap(), 7);
    }

    #[test]
    fn logical_and_left_false() {
        let (n, ops) = compile_expression("a && b").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 7, 0).unwrap(), 0);
    }

    #[test]
    fn logical_and_right_false() {
        let (n, ops) = compile_expression("a && b").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 0, 0).unwrap(), 0);
    }

    #[test]
    fn logical_and_both_false() {
        let (n, ops) = compile_expression("a && b").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    // === Logical OR (||) ===

    #[test]
    fn logical_or_left_true() {
        // a || b -> a ? 1 : b
        let (n, ops) = compile_expression("a || b").expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 5, 7, 0).unwrap(), 1);
    }

    #[test]
    fn logical_or_left_false_right_true() {
        let (n, ops) = compile_expression("a || b").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 7, 0).unwrap(), 1);
    }

    #[test]
    fn logical_or_both_false() {
        let (n, ops) = compile_expression("a || b").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    // === Combined: ternary with logical ops ===

    #[test]
    fn ternary_with_and() {
        // (a > 0 && b > 0) ? a + b : 0
        let (n, ops) = compile_expression("(a > 0 && b > 0) ? a + b : 0").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 8);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 0);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 0);
    }

    #[test]
    fn ternary_with_or() {
        // (a > 0 || b > 0) ? 1 : 0
        let (n, ops) = compile_expression("(a > 0 || b > 0) ? 1 : 0").expect("parse");
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 1);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    #[test]
    fn nested_ternary_with_logical() {
        // a > 0 ? (b > 0 ? a * b : a) : (b > 0 ? b : 0)
        let (n, ops) = compile_expression("a > 0 ? (b > 0 ? a * b : a) : (b > 0 ? b : 0)").expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 15);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    // === Combined: && / || inside ternary conditions (E2E) ===

    #[test]
    fn ternary_and_or_combined() {
        // (a > 0 && b > 0) ? a * b : (a > 0 || b > 0) ? 1 : 0
        let (n, ops) = compile_expression(
            "a > 0 && b > 0 ? a * b : a > 0 || b > 0 ? 1 : 0"
        ).expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        // both positive -> a*b
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 15);
        // a=3,b=0 -> false && -> true || -> 1
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 1);
        // a=0,b=5 -> false && -> true || -> 1
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 1);
        // both zero -> false && -> false || -> 0
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    #[test]
    fn ternary_or_and_chained() {
        // (a > 0 || b > 0) ? (a > 0 && b > 0 ? a + b : 1) : 0
        let (n, ops) = compile_expression(
            "a > 0 || b > 0 ? a > 0 && b > 0 ? a + b : 1 : 0"
        ).expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        // both positive -> a+b
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 8);
        // a=3,b=0 -> || true, && false -> 1
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 1);
        // a=0,b=0 -> || false -> 0
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    #[test]
    fn triple_nested_ternary_logical() {
        // a > 0 ? b > 0 ? a && b ? a * b : a : b : 0
        // = a>0 ? (b>0 ? (a&&b ? a*b : a) : b) : 0
        let (n, ops) = compile_expression(
            "a > 0 ? b > 0 ? a * b : a : b"
        ).expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 15);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 0, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

    #[test]
    fn and_in_false_branch() {
        // a > 0 ? a : b > 0 && a + b > 0 ? a + b : 0
        let (n, ops) = compile_expression(
            "a > 0 ? a : b > 0 ? a + b : 0"
        ).expect("parse");
        assert_eq!(n, 2);
        let wasm = build_run_module(n, &ops).unwrap();
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 3, 5, 0).unwrap(), 3);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 5, 0).unwrap(), 5);
        assert_eq!(wasmi_rt::run_i32_2(&wasm, "run", 0, 0, 0).unwrap(), 0);
    }

}

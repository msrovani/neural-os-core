//! WASM Bytecode Executor — stack-based VM minimal para skills.
//! Opcodes simples: push, pop, add, sub, load, store, call, br, ret.
//! no_std, sem alocação dinâmica no hot path.

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;

/// Opcodes da VM simplificada
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Push(u32),     // empilha constante
    Pop,           // desempilha
    Dup,           // duplica topo
    Dup2,          // duplica dois topo (a,b -> a,b,a,b)
    Add, Sub, Mul, // aritmética
    And, Or, Xor,  // bitwise
    Eq, Lt, Gt,    // comparação
    Not, Neg,      // unário
    Load(u32),     // carrega de memória (offset)
    Store(u32),    // armazena em memória (offset)
    Call(u32),     // chama função por índice
    Br(u32),       // salto incondicional
    BrIf(u32),     // salto condicional
    Ret,           // retorna
    Print,         // debug: imprime topo da pilha
    Halt,          // para execução
}

pub struct WasmExec {
    pub stack: Vec<u32>,
    pub memory: Vec<u8>,
    pub ip: usize,
    pub fuel: u64,
    pub running: bool,
}

impl WasmExec {
    pub fn new(mem_size: usize) -> Self {
        WasmExec { stack: Vec::new(), memory: vec![0u8; mem_size], ip: 0, fuel: 100_000, running: true }
    }

    pub fn step(&mut self, code: &[Op]) -> Result<(), &'static str> {
        if !self.running { return Err("halted"); }
        if self.fuel == 0 { return Err("out of fuel"); }
        self.fuel -= 1;

        if self.ip >= code.len() { self.running = false; return Ok(()); }
        let op = &code[self.ip];
        self.ip += 1;

        match op {
            Op::Push(v) => self.stack.push(*v),
            Op::Pop => { self.stack.pop().ok_or("empty stack")?; }
            Op::Dup => { let v = self.stack.last().ok_or("empty stack")?.clone(); self.stack.push(v); }
            Op::Dup2 => { let a = self.stack.pop().ok_or("stack underflow")?; let b = self.stack.last().ok_or("empty stack")?.clone(); self.stack.push(a); self.stack.push(b); self.stack.push(a); }
            Op::Add => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a.wrapping_add(b)); }
            Op::Sub => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a.wrapping_sub(b)); }
            Op::Mul => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a.wrapping_mul(b)); }
            Op::And => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a & b); }
            Op::Or  => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a | b); }
            Op::Xor => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a ^ b); }
            Op::Eq  => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(if a == b { 1 } else { 0 }); }
            Op::Lt  => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(if a < b { 1 } else { 0 }); }
            Op::Gt  => { let b = self.stack.pop().ok_or("stack underflow")?; let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(if a > b { 1 } else { 0 }); }
            Op::Not => { let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(!a); }
            Op::Neg => { let a = self.stack.pop().ok_or("stack underflow")?; self.stack.push(a.wrapping_neg()); }
            Op::Load(off) => {
                let off = *off as usize;
                if off + 4 > self.memory.len() { return Err("memory out of bounds"); }
                let val = u32::from_le_bytes([self.memory[off], self.memory[off+1], self.memory[off+2], self.memory[off+3]]);
                self.stack.push(val);
            }
            Op::Store(off) => {
                let val = self.stack.pop().ok_or("stack underflow")?;
                let off = *off as usize;
                if off + 4 > self.memory.len() { return Err("memory out of bounds"); }
                let bytes = val.to_le_bytes();
                self.memory[off..off+4].copy_from_slice(&bytes);
            }
            Op::Call(idx) => {
                // Call simples: salva IP e salta
                self.stack.push(self.ip as u32);
                self.ip = *idx as usize;
            }
            Op::Br(addr) => { self.ip = *addr as usize; }
            Op::BrIf(addr) => {
                let cond = self.stack.pop().ok_or("stack underflow")?;
                if cond != 0 { self.ip = *addr as usize; }
            }
            Op::Ret => {
                self.running = false;
            }
            Op::Print => {
                if let Some(v) = self.stack.last() {
                    k_nano::slog_hermes!("Wasm", "info", "print: {}", v);
                }
            }
            Op::Halt => { self.running = false; }
        }
        Ok(())
    }

    pub fn run(&mut self, code: &[Op]) -> Result<u32, &'static str> {
        self.ip = 0;
        self.running = true;
        while self.running {
            self.step(code)?;
        }
        Ok(self.stack.last().copied().unwrap_or(0))
    }
}

/// Gera bytecode para uma skill Echo simples
pub fn generate_echo_skill(name: &str) -> (Vec<Op>, String) {
    use Op::*;
    let code = vec![
        Push(0x484F), Push(0x4C41), // "HELLO" em ASCII invertido (little-endian)
        Print,
        Halt,
    ];
    let desc = alloc::format!("Skill WASM '{}': ecoa Hello World", name);
    (code, desc)
}

/// Gera bytecode para uma skill de calculadora
pub fn generate_calc_skill(a: u32, b: u32) -> Vec<Op> {
    use Op::*;
    vec![Push(a), Push(b), Add, Print, Halt]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add() {
        let mut vm = WasmExec::new(256);
        let code = vec![Op::Push(3), Op::Push(4), Op::Add, Op::Halt];
        assert_eq!(vm.run(&code).unwrap(), 7);
    }
    #[test]
    fn test_compare() {
        let mut vm = WasmExec::new(256);
        let code = vec![Op::Push(5), Op::Push(3), Op::Gt, Op::Halt];
        assert_eq!(vm.run(&code).unwrap(), 1);
    }
}

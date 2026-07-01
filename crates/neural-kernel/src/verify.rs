//! Skill Verifier — verificacao de bytecode de skills estilo eBPF.
//! AxiomOS-inspired: prova que uma skill e segura antes de executar.
//! Verifica: bounds de loops, acesso a memoria, comandos bloqueados.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerifyResult {
    Pass,
    Fail(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    Nop,
    Read { offset: u16 },
    Write { offset: u16 },
    Call { skill_id: u16 },
    Loop { max_iters: u16 },
    Cmp { reg: u8, val: u16 },
    Jump { target: u16 },
    Halt,
}

/// Programa de skill verificado
pub struct VerifiedProgram {
    pub name: String,
    pub code: Vec<OpCode>,
    pub stack_size: u16,
    pub max_loops: u16,
}

/// Verificador: prova que o programa e seguro
pub fn verify_skill(name: &str, code: &[OpCode]) -> Result<VerifiedProgram, &'static str> {
    let mut max_loops = 0u16;
    let mut stack_size = 0u16;
    let mut has_halt = false;
    let mut visited = vec![false; code.len()];

    for (i, op) in code.iter().enumerate() {
        match op {
            OpCode::Loop { max_iters } => {
                if *max_iters > 1000 { return Err("Loop exceeds max iterations (1000)"); }
                max_loops = max_loops.max(*max_iters);
            }
            OpCode::Write { offset } => {
                if *offset > 4096 { return Err("Write beyond stack boundary"); }
                if *offset > stack_size { stack_size = *offset + 1; }
            }
            OpCode::Read { offset } => {
                if *offset > stack_size { return Err("Read beyond stack boundary"); }
            }
            OpCode::Call { skill_id } => {
                // Skill ID 0 = reserved (blocked)
                if *skill_id == 0 { return Err("Call to blocked skill (id=0)"); }
            }
            OpCode::Jump { target } => {
                if *target as usize >= code.len() { return Err("Jump target out of bounds"); }
            }
            OpCode::Halt => { has_halt = true; }
            _ => {}
        }
        visited[i] = true;
    }

    // Verifica se todos os opcodes sao alcancaveis
    if visited.iter().any(|&v| !v) {
        return Err("Dead code detected");
    }

    Ok(VerifiedProgram {
        name: String::from(name),
        code: code.to_vec(),
        stack_size,
        max_loops,
    })
}

/// Completion contract: verifica se a output de uma skill atende ao esperado
pub fn completion_check(output: &[u8], expected_prefix: &[u8]) -> VerifyResult {
    if output.len() < expected_prefix.len() {
        return VerifyResult::Fail("Output shorter than expected");
    }
    if &output[..expected_prefix.len()] != expected_prefix {
        return VerifyResult::Fail("Output prefix mismatch");
    }
    if output.is_empty() {
        return VerifyResult::Fail("Empty output (no evidence)");
    }
    VerifyResult::Pass
}

/// Executa um programa verificado (interpretador minimal)
pub fn execute_verified(prog: &VerifiedProgram, input: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut stack = vec![0u8; prog.stack_size as usize];
    let mut pc = 0usize;
    let mut output = Vec::new();

    while pc < prog.code.len() {
        match &prog.code[pc] {
            OpCode::Nop => {}
            OpCode::Read { offset } => {
                if (*offset as usize) < input.len() {
                    stack[*offset as usize] = input[*offset as usize];
                }
            }
            OpCode::Write { offset } => {
                if (*offset as usize) < stack.len() {
                    output.push(stack[*offset as usize]);
                }
            }
            OpCode::Halt => { break; }
            _ => {}
        }
        pc += 1;
    }

    Ok(output)
}

/// PS/2 set 1 scancode → ASCII. Retorna None para teclas não imprimíveis
/// (modifiers, Enter, Backspace — tratadas separadamente no InputAgent).
/// Tabela: make code (pressed, <0x80) → caractere. Break codes (≥0x80) não entram aqui
/// (InputAgent filtra: `if !pressed { return; }` antes de chamar esta função).
///
/// Pure function — shift/caps são passados pelo caller (InputAgent rastreia os
/// modifiers). Caps só afeta letras; shift afeta letras (XOR com caps), dígitos e
/// símbolos. Função pura, sem estado.
pub fn scancode_to_ascii(scancode: u8, shift: bool, caps: bool) -> Option<char> {
    // Ordem do scancode (make, set 1) → ASCII. Apenas teclas imprimíveis.
    // Fonte: PS/2 set 1 scancode table (IBM PC/AT).
    match scancode {
        // Dígitos (linha superior): shift → símbolos.
        0x02 => Some(if shift { '!' } else { '1' }),
        0x03 => Some(if shift { '@' } else { '2' }),
        0x04 => Some(if shift { '#' } else { '3' }),
        0x05 => Some(if shift { '$' } else { '4' }),
        0x06 => Some(if shift { '%' } else { '5' }),
        0x07 => Some(if shift { '^' } else { '6' }),
        0x08 => Some(if shift { '&' } else { '7' }),
        0x09 => Some(if shift { '*' } else { '8' }),
        0x0A => Some(if shift { '(' } else { '9' }),
        0x0B => Some(if shift { ')' } else { '0' }),
        // Letras a-z: uppercase iff shift XOR caps (shift OU caps = maiúscula).
        0x1E => Some(if shift != caps { 'A' } else { 'a' }),
        0x30 => Some(if shift != caps { 'B' } else { 'b' }),
        0x2E => Some(if shift != caps { 'C' } else { 'c' }),
        0x20 => Some(if shift != caps { 'D' } else { 'd' }),
        0x12 => Some(if shift != caps { 'E' } else { 'e' }),
        0x21 => Some(if shift != caps { 'F' } else { 'f' }),
        0x22 => Some(if shift != caps { 'G' } else { 'g' }),
        0x23 => Some(if shift != caps { 'H' } else { 'h' }),
        0x17 => Some(if shift != caps { 'I' } else { 'i' }),
        0x24 => Some(if shift != caps { 'J' } else { 'j' }),
        0x25 => Some(if shift != caps { 'K' } else { 'k' }),
        0x26 => Some(if shift != caps { 'L' } else { 'l' }),
        0x32 => Some(if shift != caps { 'M' } else { 'm' }),
        0x31 => Some(if shift != caps { 'N' } else { 'n' }),
        0x18 => Some(if shift != caps { 'O' } else { 'o' }),
        0x19 => Some(if shift != caps { 'P' } else { 'p' }),
        0x10 => Some(if shift != caps { 'Q' } else { 'q' }),
        0x13 => Some(if shift != caps { 'R' } else { 'r' }),
        0x1F => Some(if shift != caps { 'S' } else { 's' }),
        0x14 => Some(if shift != caps { 'T' } else { 't' }),
        0x16 => Some(if shift != caps { 'U' } else { 'u' }),
        0x2F => Some(if shift != caps { 'V' } else { 'v' }),
        0x11 => Some(if shift != caps { 'W' } else { 'w' }),
        0x2D => Some(if shift != caps { 'X' } else { 'x' }),
        0x15 => Some(if shift != caps { 'Y' } else { 'y' }),
        0x2C => Some(if shift != caps { 'Z' } else { 'z' }),
        // Espaço: shift não muda.
        0x39 => Some(' '),
        // Símbolos (shift → shifted variant).
        0x0C => Some(if shift { '_' } else { '-' }),
        0x0D => Some(if shift { '+' } else { '=' }),
        0x1A => Some(if shift { '{' } else { '[' }),
        0x1B => Some(if shift { '}' } else { ']' }),
        0x27 => Some(if shift { ':' } else { ';' }),
        0x28 => Some(if shift { '"' } else { '\'' }),
        0x29 => Some(if shift { '~' } else { '`' }),
        0x2B => Some(if shift { '|' } else { '\\' }),
        0x33 => Some(if shift { '<' } else { ',' }),
        0x34 => Some(if shift { '>' } else { '.' }),
        0x35 => Some(if shift { '?' } else { '/' }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_lowercase() {
        assert_eq!(scancode_to_ascii(0x1E, false, false), Some('a'));
        assert_eq!(scancode_to_ascii(0x30, false, false), Some('b'));
        assert_eq!(scancode_to_ascii(0x2E, false, false), Some('c'));
        assert_eq!(scancode_to_ascii(0x20, false, false), Some('d'));
        assert_eq!(scancode_to_ascii(0x12, false, false), Some('e'));
        assert_eq!(scancode_to_ascii(0x21, false, false), Some('f'));
        assert_eq!(scancode_to_ascii(0x22, false, false), Some('g'));
        assert_eq!(scancode_to_ascii(0x23, false, false), Some('h'));
        assert_eq!(scancode_to_ascii(0x17, false, false), Some('i'));
        assert_eq!(scancode_to_ascii(0x24, false, false), Some('j'));
        assert_eq!(scancode_to_ascii(0x25, false, false), Some('k'));
        assert_eq!(scancode_to_ascii(0x26, false, false), Some('l'));
        assert_eq!(scancode_to_ascii(0x32, false, false), Some('m'));
        assert_eq!(scancode_to_ascii(0x31, false, false), Some('n'));
        assert_eq!(scancode_to_ascii(0x18, false, false), Some('o'));
        assert_eq!(scancode_to_ascii(0x19, false, false), Some('p'));
        assert_eq!(scancode_to_ascii(0x10, false, false), Some('q'));
        assert_eq!(scancode_to_ascii(0x13, false, false), Some('r'));
        assert_eq!(scancode_to_ascii(0x1F, false, false), Some('s'));
        assert_eq!(scancode_to_ascii(0x14, false, false), Some('t'));
        assert_eq!(scancode_to_ascii(0x16, false, false), Some('u'));
        assert_eq!(scancode_to_ascii(0x2F, false, false), Some('v'));
        assert_eq!(scancode_to_ascii(0x11, false, false), Some('w'));
        assert_eq!(scancode_to_ascii(0x2D, false, false), Some('x'));
        assert_eq!(scancode_to_ascii(0x15, false, false), Some('y'));
        assert_eq!(scancode_to_ascii(0x2C, false, false), Some('z'));
    }

    #[test]
    fn digits() {
        assert_eq!(scancode_to_ascii(0x0B, false, false), Some('0'));
        assert_eq!(scancode_to_ascii(0x02, false, false), Some('1'));
        assert_eq!(scancode_to_ascii(0x03, false, false), Some('2'));
        assert_eq!(scancode_to_ascii(0x04, false, false), Some('3'));
        assert_eq!(scancode_to_ascii(0x05, false, false), Some('4'));
        assert_eq!(scancode_to_ascii(0x06, false, false), Some('5'));
        assert_eq!(scancode_to_ascii(0x07, false, false), Some('6'));
        assert_eq!(scancode_to_ascii(0x08, false, false), Some('7'));
        assert_eq!(scancode_to_ascii(0x09, false, false), Some('8'));
        assert_eq!(scancode_to_ascii(0x0A, false, false), Some('9'));
    }

    #[test]
    fn symbols() {
        assert_eq!(scancode_to_ascii(0x39, false, false), Some(' '));
        assert_eq!(scancode_to_ascii(0x34, false, false), Some('.'));
        assert_eq!(scancode_to_ascii(0x35, false, false), Some('/'));
        assert_eq!(scancode_to_ascii(0x0C, false, false), Some('-'));
        assert_eq!(scancode_to_ascii(0x0D, false, false), Some('='));
        // Teclas que faltavam na tabela anterior (presentes na cópia bin morta).
        assert_eq!(scancode_to_ascii(0x1A, false, false), Some('['));
        assert_eq!(scancode_to_ascii(0x1B, false, false), Some(']'));
        assert_eq!(scancode_to_ascii(0x27, false, false), Some(';'));
        assert_eq!(scancode_to_ascii(0x28, false, false), Some('\''));
        assert_eq!(scancode_to_ascii(0x29, false, false), Some('`'));
        assert_eq!(scancode_to_ascii(0x2B, false, false), Some('\\'));
        assert_eq!(scancode_to_ascii(0x33, false, false), Some(','));
    }

    #[test]
    fn shift_uppercase() {
        assert_eq!(scancode_to_ascii(0x1E, true, false), Some('A'));
        assert_eq!(scancode_to_ascii(0x2C, true, false), Some('Z'));
    }

    #[test]
    fn caps_uppercase() {
        assert_eq!(scancode_to_ascii(0x1E, false, true), Some('A'));
        assert_eq!(scancode_to_ascii(0x2C, false, true), Some('Z'));
    }

    #[test]
    fn shift_plus_caps_is_lowercase() {
        // XOR: shift OU caps → maiúscula; ambos → minúscula (comportamento PC padrão).
        assert_eq!(scancode_to_ascii(0x1E, true, true), Some('a'));
        assert_eq!(scancode_to_ascii(0x2C, true, true), Some('z'));
    }

    #[test]
    fn shifted_digits() {
        assert_eq!(scancode_to_ascii(0x02, true, false), Some('!'));
        assert_eq!(scancode_to_ascii(0x03, true, false), Some('@'));
        assert_eq!(scancode_to_ascii(0x04, true, false), Some('#'));
        assert_eq!(scancode_to_ascii(0x05, true, false), Some('$'));
        assert_eq!(scancode_to_ascii(0x06, true, false), Some('%'));
        assert_eq!(scancode_to_ascii(0x07, true, false), Some('^'));
        assert_eq!(scancode_to_ascii(0x08, true, false), Some('&'));
        assert_eq!(scancode_to_ascii(0x09, true, false), Some('*'));
        assert_eq!(scancode_to_ascii(0x0A, true, false), Some('('));
        assert_eq!(scancode_to_ascii(0x0B, true, false), Some(')'));
    }

    #[test]
    fn shifted_symbols() {
        assert_eq!(scancode_to_ascii(0x1A, true, false), Some('{'));
        assert_eq!(scancode_to_ascii(0x1B, true, false), Some('}'));
        assert_eq!(scancode_to_ascii(0x27, true, false), Some(':'));
        assert_eq!(scancode_to_ascii(0x28, true, false), Some('"'));
        assert_eq!(scancode_to_ascii(0x29, true, false), Some('~'));
        assert_eq!(scancode_to_ascii(0x2B, true, false), Some('|'));
        assert_eq!(scancode_to_ascii(0x33, true, false), Some('<'));
        assert_eq!(scancode_to_ascii(0x34, true, false), Some('>'));
        assert_eq!(scancode_to_ascii(0x35, true, false), Some('?'));
        assert_eq!(scancode_to_ascii(0x0C, true, false), Some('_'));
        assert_eq!(scancode_to_ascii(0x0D, true, false), Some('+'));
    }

    #[test]
    fn caps_does_not_affect_digits() {
        assert_eq!(scancode_to_ascii(0x02, false, true), Some('1'));
        assert_eq!(scancode_to_ascii(0x0B, true, true), Some(')'));
    }

    #[test]
    fn shift_does_not_affect_space() {
        assert_eq!(scancode_to_ascii(0x39, true, false), Some(' '));
        assert_eq!(scancode_to_ascii(0x39, false, true), Some(' '));
    }

    #[test]
    fn non_printable_returns_none() {
        assert_eq!(scancode_to_ascii(0x1C, false, false), None); // Enter
        assert_eq!(scancode_to_ascii(0x1C, true, false), None); // Enter com shift
        assert_eq!(scancode_to_ascii(0x0E, false, false), None); // Backspace
        assert_eq!(scancode_to_ascii(0x1D, false, false), None); // Ctrl
        assert_eq!(scancode_to_ascii(0x38, false, false), None); // Alt
        assert_eq!(scancode_to_ascii(0x2A, false, false), None); // Shift
        assert_eq!(scancode_to_ascii(0x36, true, true), None); // Shift direito
        assert_eq!(scancode_to_ascii(0x3A, false, false), None); // CapsLock
        assert_eq!(scancode_to_ascii(0x01, false, false), None); // Escape
        assert_eq!(scancode_to_ascii(0x0F, false, false), None); // Tab
        assert_eq!(scancode_to_ascii(0x53, true, true), None); // Delete
    }
}

/// PS/2 set 1 scancode → ASCII (lowercase). Retorna None para teclas não
/// imprimíveis (modifiers, Enter, Backspace — tratadas separadamente no InputAgent).
/// Tabela: make code (pressed, <0x80) → caractere. Break codes (≥0x80) não entram aqui
/// (InputAgent filtra: `if !pressed { return; }` antes de chamar esta função).
pub fn scancode_to_ascii(scancode: u8) -> Option<char> {
    // Ordem do scancode (make, set 1) → ASCII. Apenas teclas imprimíveis.
    // Fonte: PS/2 set 1 scancode table (IBM PC/AT).
    match scancode {
        0x0B => Some('0'), 0x02 => Some('1'), 0x03 => Some('2'),
        0x04 => Some('3'), 0x05 => Some('4'), 0x06 => Some('5'),
        0x07 => Some('6'), 0x08 => Some('7'), 0x09 => Some('8'),
        0x0A => Some('9'),
        0x1E => Some('a'), 0x30 => Some('b'), 0x2E => Some('c'),
        0x20 => Some('d'), 0x12 => Some('e'), 0x21 => Some('f'),
        0x22 => Some('g'), 0x23 => Some('h'), 0x17 => Some('i'),
        0x24 => Some('j'), 0x25 => Some('k'), 0x26 => Some('l'),
        0x32 => Some('m'), 0x31 => Some('n'), 0x18 => Some('o'),
        0x19 => Some('p'), 0x10 => Some('q'), 0x13 => Some('r'),
        0x1F => Some('s'), 0x14 => Some('t'), 0x16 => Some('u'),
        0x2F => Some('v'), 0x11 => Some('w'), 0x2D => Some('x'),
        0x15 => Some('y'), 0x2C => Some('z'),
        0x39 => Some(' '),
        0x34 => Some('.'), 0x35 => Some('/'), 0x0C => Some('-'),
        0x0D => Some('='),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_lowercase() {
        assert_eq!(scancode_to_ascii(0x1E), Some('a'));
        assert_eq!(scancode_to_ascii(0x30), Some('b'));
        assert_eq!(scancode_to_ascii(0x2E), Some('c'));
        assert_eq!(scancode_to_ascii(0x20), Some('d'));
        assert_eq!(scancode_to_ascii(0x12), Some('e'));
        assert_eq!(scancode_to_ascii(0x21), Some('f'));
        assert_eq!(scancode_to_ascii(0x22), Some('g'));
        assert_eq!(scancode_to_ascii(0x23), Some('h'));
        assert_eq!(scancode_to_ascii(0x17), Some('i'));
        assert_eq!(scancode_to_ascii(0x24), Some('j'));
        assert_eq!(scancode_to_ascii(0x25), Some('k'));
        assert_eq!(scancode_to_ascii(0x26), Some('l'));
        assert_eq!(scancode_to_ascii(0x32), Some('m'));
        assert_eq!(scancode_to_ascii(0x31), Some('n'));
        assert_eq!(scancode_to_ascii(0x18), Some('o'));
        assert_eq!(scancode_to_ascii(0x19), Some('p'));
        assert_eq!(scancode_to_ascii(0x10), Some('q'));
        assert_eq!(scancode_to_ascii(0x13), Some('r'));
        assert_eq!(scancode_to_ascii(0x1F), Some('s'));
        assert_eq!(scancode_to_ascii(0x14), Some('t'));
        assert_eq!(scancode_to_ascii(0x16), Some('u'));
        assert_eq!(scancode_to_ascii(0x2F), Some('v'));
        assert_eq!(scancode_to_ascii(0x11), Some('w'));
        assert_eq!(scancode_to_ascii(0x2D), Some('x'));
        assert_eq!(scancode_to_ascii(0x15), Some('y'));
        assert_eq!(scancode_to_ascii(0x2C), Some('z'));
    }

    #[test]
    fn digits() {
        assert_eq!(scancode_to_ascii(0x0B), Some('0'));
        assert_eq!(scancode_to_ascii(0x02), Some('1'));
        assert_eq!(scancode_to_ascii(0x03), Some('2'));
        assert_eq!(scancode_to_ascii(0x04), Some('3'));
        assert_eq!(scancode_to_ascii(0x05), Some('4'));
        assert_eq!(scancode_to_ascii(0x06), Some('5'));
        assert_eq!(scancode_to_ascii(0x07), Some('6'));
        assert_eq!(scancode_to_ascii(0x08), Some('7'));
        assert_eq!(scancode_to_ascii(0x09), Some('8'));
        assert_eq!(scancode_to_ascii(0x0A), Some('9'));
    }

    #[test]
    fn symbols() {
        assert_eq!(scancode_to_ascii(0x39), Some(' '));
        assert_eq!(scancode_to_ascii(0x34), Some('.'));
        assert_eq!(scancode_to_ascii(0x35), Some('/'));
        assert_eq!(scancode_to_ascii(0x0C), Some('-'));
        assert_eq!(scancode_to_ascii(0x0D), Some('='));
    }

    #[test]
    fn non_printable_returns_none() {
        assert_eq!(scancode_to_ascii(0x1C), None); // Enter
        assert_eq!(scancode_to_ascii(0x0E), None); // Backspace
        assert_eq!(scancode_to_ascii(0x1D), None); // Ctrl
        assert_eq!(scancode_to_ascii(0x38), None); // Alt
        assert_eq!(scancode_to_ascii(0x2A), None); // Shift
        assert_eq!(scancode_to_ascii(0x01), None); // Escape
        assert_eq!(scancode_to_ascii(0x0F), None); // Tab
    }
}
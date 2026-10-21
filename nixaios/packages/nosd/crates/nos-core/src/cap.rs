//! CapGate bitmask — alinhado à semântica hermes wasmi_rt (PORT).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapBits(pub u32);

impl CapBits {
    pub const NONE: Self = Self(0);
    pub const INFER: Self = Self(1 << 0);
    pub const NET: Self = Self(1 << 1);
    pub const FS: Self = Self(1 << 2);
    pub const AGENT_SPAWN: Self = Self(1 << 3);
    pub const MEMORY: Self = Self(1 << 4);
    pub const NIX_WRITE: Self = Self(1 << 5);
    pub const ALL: Self = Self(0xFFFF_FFFF);

    pub fn contains(self, required: CapBits) -> bool {
        (self.0 & required.0) == required.0
    }
}

#[derive(Debug, Clone)]
pub struct CapGate {
    pub held: CapBits,
    pub generation: u64,
}

impl CapGate {
    pub fn new(held: CapBits, generation: u64) -> Self {
        Self { held, generation }
    }

    pub fn allow(&self, required: CapBits, current_gen: u64) -> Result<(), &'static str> {
        if self.generation != current_gen {
            return Err("capability generation revoked");
        }
        if !self.held.contains(required) {
            return Err("capability denied (bitmask)");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_without_bit() {
        let g = CapGate::new(CapBits::INFER, 1);
        assert!(g.allow(CapBits::NET, 1).is_err());
        assert!(g.allow(CapBits::INFER, 1).is_ok());
    }

    #[test]
    fn revoke_on_gen_bump() {
        let g = CapGate::new(CapBits::ALL, 1);
        assert!(g.allow(CapBits::INFER, 2).is_err());
    }
}

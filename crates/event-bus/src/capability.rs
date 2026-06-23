#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityToken(pub u64);

impl CapabilityToken {
    pub fn is_valid(&self) -> bool {
        self.0 > 0
    }
}

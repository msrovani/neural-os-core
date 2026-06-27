#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPayload {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

/// Token de capacidade: mantém compatibilidade com u64 legado,
/// mas agora pode transportar identidade Ed25519.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityToken {
    Legacy(u64),
    Ed25519(IdentityPayload),
}

impl CapabilityToken {
    pub fn is_valid(&self) -> bool {
        match self {
            CapabilityToken::Legacy(val) => *val > 0,
            CapabilityToken::Ed25519(_) => true,
        }
    }

    pub fn as_legacy(&self) -> u64 {
        match self {
            CapabilityToken::Legacy(val) => *val,
            CapabilityToken::Ed25519(_) => 1,
        }
    }
}

/// Conveniência: construtor para token legado
impl From<u64> for CapabilityToken {
    fn from(val: u64) -> Self {
        CapabilityToken::Legacy(val)
    }
}


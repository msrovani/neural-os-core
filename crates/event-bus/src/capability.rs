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
            // Auditoria 6.3: IdentityPayload carrega (public_key, signature) sem
            // mensagem vinculada — não há como verificar a assinatura aqui (crate
            // leaf, sem crypto) e o tráfego real usa Legacy. Negação por padrão:
            // token não verificável é inválido (fail-closed), nunca `true` cego.
            CapabilityToken::Ed25519(_) => false,
        }
    }

    pub fn as_legacy(&self) -> u64 {
        match self {
            CapabilityToken::Legacy(val) => *val,
            // Fail-closed: never impersonate Legacy(1). Callers that gate on
            // as_legacy() + required_tokens must see 0 for unverifiable Ed25519.
            CapabilityToken::Ed25519(_) => 0,
        }
    }
}

/// Conveniência: construtor para token legado
impl From<u64> for CapabilityToken {
    fn from(val: u64) -> Self {
        CapabilityToken::Legacy(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ed25519_never_as_legacy_one() {
        let t = CapabilityToken::Ed25519(IdentityPayload {
            public_key: [0; 32],
            signature: [0; 64],
        });
        assert!(!t.is_valid());
        assert_eq!(t.as_legacy(), 0);
    }

    #[test]
    fn legacy_zero_invalid() {
        assert!(!CapabilityToken::Legacy(0).is_valid());
        assert!(CapabilityToken::Legacy(1).is_valid());
    }
}


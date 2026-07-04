use alloc::vec::Vec;

pub type ValidationFn = fn(&[u8]) -> Result<(), &'static str>;

pub struct CompletionContract {
    pub name: &'static str,
    pub validate: ValidationFn,
    pub on_failure: ContractAction,
}

pub enum ContractAction {
    WarnOnly,
    RejectOutput,
    RetrySkill,
}

impl CompletionContract {
    pub fn verify(&self, output: &[u8]) -> Result<(), &'static str> {
        (self.validate)(output)
    }
}

pub const CONTRACT_NONEMPTY: CompletionContract = CompletionContract {
    name: "non_empty",
    validate: |out| {
        if out.is_empty() { Err("output vazio") } else { Ok(()) }
    },
    on_failure: ContractAction::WarnOnly,
};

pub const CONTRACT_UTF8: CompletionContract = CompletionContract {
    name: "utf8",
    validate: |out| {
        if core::str::from_utf8(out).is_ok() { Ok(()) } else { Err("output nao e utf-8 valido") }
    },
    on_failure: ContractAction::RejectOutput,
};

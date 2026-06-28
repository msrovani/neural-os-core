use alloc::string::String;
use alloc::vec::Vec;

pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    pub tokens: u16,
    pub domains: Vec<String>,
}

pub struct SkillIndex {
    pub skills: Vec<SkillFrontmatter>,
}

impl SkillIndex {
    pub fn new() -> Self { SkillIndex { skills: Vec::new() } }

    pub fn register(&mut self, name: &str, description: &str, tokens: u16, domains: &[&str]) {
        self.skills.push(SkillFrontmatter {
            name: String::from(name), description: String::from(description),
            tokens, domains: domains.iter().map(|d| String::from(*d)).collect(),
        });
    }

    pub fn scan(&self, query: &str) -> Vec<&SkillFrontmatter> {
        let q = query.to_ascii_lowercase();
        let mut results: Vec<_> = self.skills.iter().filter(|s| {
            s.description.to_ascii_lowercase().contains(&q)
                || s.domains.iter().any(|d| d.contains(&q))
        }).collect();
        results.sort_by(|a, b| b.tokens.cmp(&a.tokens));
        results.truncate(5);
        results
    }
}

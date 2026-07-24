//! Tokenização + stopwords EN + PT-BR (ADR-0064 §2.2).

use alloc::string::String;
use alloc::vec::Vec;

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        // EN
        "the" | "is" | "at" | "of" | "on" | "in" | "to" | "for" | "and" | "or" | "an" | "as"
            | "it" | "be" | "by" | "if" | "no" | "so" | "do" | "he" | "we" | "my" | "up" | "am"
            | "me" | "us" | "not" | "but" | "are" | "was" | "has" | "had" | "its" | "can" | "may"
            | "our" | "you" | "all" | "any" | "who" | "how" | "did" | "get" | "got" | "set" | "let"
            | "new" | "old" | "use" | "now" | "way" | "own" | "see" | "say" | "her" | "him" | "his"
            | "she" | "they" | "them" | "this" | "that" | "with" | "from" | "have" | "been" | "were"
            | "will" | "what" | "when" | "your" | "than" | "each" | "just" | "also" | "into" | "over"
            | "such" | "some" | "very" | "only" | "then" | "more" | "about" | "which" | "would"
            | "could" | "should" | "there" | "their" | "these" | "those" | "other"
            // PT-BR
            | "o" | "a" | "de" | "que" | "em" | "um" | "para" | "com" | "nao" | "não" | "uma"
            | "os" | "dos" | "das" | "ao" | "aos" | "pelo" | "pela" | "seu" | "sua" | "mais"
            | "mas" | "nem" | "tambem" | "também" | "ja" | "já" | "quando" | "onde" | "como"
            | "porque" | "entao" | "então"
    )
}

/// Lowercase, split non-alnum (keep `_` `-`), len>=2, drop stopwords.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || lower == '_' || lower == '-' {
            cur.push(lower);
        } else if !cur.is_empty() {
            if cur.len() >= 2 && !is_stopword(cur.as_str()) {
                out.push(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.len() >= 2 && !is_stopword(cur.as_str()) {
        out.push(cur);
    }
    out
}

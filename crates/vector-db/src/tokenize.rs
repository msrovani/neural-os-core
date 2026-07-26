use alloc::string::String;
use alloc::vec::Vec;

/// EN stopwords (most common ~80).
const EN_STOPWORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and",
    "any", "are", "arent", "as", "at", "be", "because", "been", "before", "being",
    "below", "between", "both", "but", "by", "cant", "cannot", "could", "couldnt",
    "did", "didnt", "do", "does", "doesnt", "doing", "dont", "down", "during",
    "each", "few", "for", "from", "further", "had", "hadnt", "has", "hasnt", "have",
    "havent", "having", "he", "hed", "hell", "hes", "her", "here", "heres", "hers",
    "herself", "him", "himself", "his", "how", "hows", "i", "id", "ill", "im",
    "ive", "if", "in", "into", "is", "isnt", "it", "its", "itself", "lets", "me",
    "more", "most", "mustnt", "my", "myself", "no", "nor", "not", "of", "off",
    "on", "once", "only", "or", "other", "ought", "our", "ours", "ourselves", "out",
    "over", "own", "same", "shant", "she", "shed", "shell", "shes", "should",
    "shouldnt", "so", "some", "such", "than", "that", "thats", "the", "their",
    "theirs", "them", "themselves", "then", "there", "theres", "these", "they",
    "theyd", "theyll", "theyre", "theyve", "this", "those", "through", "to", "too",
    "under", "until", "up", "very", "was", "wasnt", "we", "wed", "well", "were",
    "weve", "were", "werent", "what", "whats", "when", "whens", "where", "wheres",
    "which", "while", "who", "whos", "whom", "why", "whys", "will", "with", "wont",
    "would", "wouldnt", "you", "youd", "youll", "youre", "youve", "your", "yours",
    "yourself", "yourselves",
];

/// PT-BR stopwords (most common ~30).
const PT_STOPWORDS: &[&str] = &[
    "a", "ao", "aos", "aquela", "aquelas", "aquele", "aqueles", "aquilo", "as",
    "ate", "com", "como", "da", "das", "de", "dela", "delas", "dele", "deles",
    "depois", "do", "dos", "e", "ela", "elas", "ele", "eles", "em", "entre", "era",
    "eram", "essa", "essas", "esse", "esses", "esta", "estas", "este", "estes",
    "eu", "foi", "foram", "ha", "isso", "isto", "ja", "la", "lhe", "lhes", "mais",
    "mas", "me", "mesmo", "meu", "meus", "minha", "minhas", "muito", "na", "nao",
    "nas", "nem", "no", "nos", "nossa", "nossas", "nosso", "nossos", "num", "numa",
    "o", "os", "ou", "para", "pela", "pelas", "pelo", "pelos", "por", "qual",
    "quando", "que", "quem", "se", "sem", "seu", "seus", "sua", "suas", "talvez",
    "te", "tem", "temos", "tenho", "teu", "teus", "ti", "tua", "tuas", "tu", "um",
    "uma", "umas", "uns", "voce", "voces",
];

fn is_stopword(word: &str) -> bool {
    // ponytail: linear scan across ~140 words is fine at typical doc/query sizes
    EN_STOPWORDS.contains(&word) || PT_STOPWORDS.contains(&word)
}

/// Tokenize text into words: lowercase, split by non-alphanumeric, filter stopwords.
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();
    let mut current = alloc::string::String::new();

    for ch in lower.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else {
            if !current.is_empty() {
                let word = alloc::string::String::from(current.as_str());
                if !is_stopword(&word) {
                    tokens.push(word);
                }
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        let word = alloc::string::String::from(current.as_str());
        if !is_stopword(&word) {
            tokens.push(word);
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn demo() -> bool {
        let t = tokenize("Hello World! This is a test of the emergency broadcast system.");
        // "hello", "world", "test", "emergency", "broadcast", "system" (stopwords removed)
        assert!(!t.contains(&"this".into()));
        assert!(!t.contains(&"is".into()));
        assert!(!t.contains(&"a".into()));
        assert!(!t.contains(&"the".into()));
        assert!(t.contains(&"hello".into()));
        assert!(t.contains(&"world".into()));
        assert!(t.contains(&"test".into()));
        true
    }
}

//! Text colouring for the diff view: a small syntax highlighter (keywords, types, strings,
//! comments, numbers) for common languages, chosen by file extension, and word-level change
//! highlights between a removed line and the added line that replaced it.
//!
//! Lines are coloured one at a time (a block comment opened on an earlier line isn't tracked):
//! cheap enough for every visible row, and good enough to read a diff by.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Type,
    Str,
    Comment,
    Number,
}

/// What a language looks like to the highlighter.
pub struct Lang {
    pub name: &'static str,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    /// Identifiers starting with an upper-case letter are types.
    capital_types: bool,
}

const C_LIKE_TYPES: &[&str] = &["int", "char", "float", "double", "void", "bool", "long", "short", "unsigned", "signed", "size_t"];

static RUST: Lang = Lang {
    name: "Rust",
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
        "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while",
    ],
    types: &["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "bool", "char", "str"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"'],
    capital_types: true,
};

static C_FAMILY: Lang = Lang {
    name: "C",
    keywords: &[
        "auto",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "constexpr",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "explicit",
        "extern",
        "false",
        "for",
        "friend",
        "goto",
        "if",
        "inline",
        "namespace",
        "new",
        "nullptr",
        "operator",
        "private",
        "protected",
        "public",
        "return",
        "sizeof",
        "static",
        "struct",
        "switch",
        "template",
        "this",
        "throw",
        "true",
        "try",
        "typedef",
        "typename",
        "union",
        "using",
        "virtual",
        "volatile",
        "while",
        "#include",
        "#define",
        "#if",
        "#ifdef",
        "#ifndef",
        "#endif",
        "#else",
        "#pragma",
    ],
    types: C_LIKE_TYPES,
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    capital_types: true,
};

static JAVA_LIKE: Lang = Lang {
    name: "Java",
    keywords: &[
        "abstract",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "data",
        "default",
        "do",
        "else",
        "enum",
        "extends",
        "false",
        "final",
        "finally",
        "for",
        "fun",
        "func",
        "go",
        "if",
        "implements",
        "import",
        "in",
        "interface",
        "internal",
        "is",
        "let",
        "new",
        "null",
        "object",
        "override",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "struct",
        "super",
        "switch",
        "this",
        "throw",
        "throws",
        "true",
        "try",
        "val",
        "var",
        "void",
        "when",
        "while",
        "defer",
        "chan",
        "select",
        "range",
        "map",
        "type",
        "nil",
        "guard",
        "self",
        "async",
        "await",
        "using",
        "namespace",
        "readonly",
        "sealed",
    ],
    types: &["int", "long", "short", "byte", "char", "float", "double", "boolean", "string", "String", "bool", "error", "any"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\'', '`'],
    capital_types: true,
};

static JS: Lang = Lang {
    name: "JavaScript",
    keywords: &[
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "of",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "type",
        "typeof",
        "undefined",
        "var",
        "void",
        "while",
        "yield",
        "enum",
        "implements",
        "private",
        "public",
        "protected",
        "readonly",
        "as",
    ],
    types: &["string", "number", "boolean", "any", "unknown", "never", "object", "bigint", "symbol"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\'', '`'],
    capital_types: true,
};

static PYTHON: Lang = Lang {
    name: "Python",
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for",
        "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try",
        "while", "with", "yield", "match", "case",
    ],
    types: &["int", "float", "str", "bool", "list", "dict", "tuple", "set", "bytes", "object"],
    line_comments: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    capital_types: true,
};

static RUBY: Lang = Lang {
    name: "Ruby",
    keywords: &[
        "begin",
        "break",
        "case",
        "class",
        "def",
        "do",
        "else",
        "elsif",
        "end",
        "ensure",
        "false",
        "for",
        "if",
        "in",
        "module",
        "next",
        "nil",
        "not",
        "or",
        "and",
        "redo",
        "rescue",
        "retry",
        "return",
        "self",
        "super",
        "then",
        "true",
        "unless",
        "until",
        "when",
        "while",
        "yield",
        "require",
        "attr_reader",
        "attr_accessor",
    ],
    types: &[],
    line_comments: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    capital_types: true,
};

static SHELL: Lang = Lang {
    name: "Shell",
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in", "function", "return", "local", "export",
        "set", "unset", "echo", "exit", "source", "readonly", "shift",
    ],
    types: &[],
    line_comments: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    capital_types: false,
};

static CONFIG: Lang = Lang {
    name: "Config",
    keywords: &["true", "false", "null", "yes", "no", "on", "off"],
    types: &[],
    line_comments: &["#", ";"],
    block_comment: None,
    quotes: &['"', '\''],
    capital_types: false,
};

static JSON: Lang = Lang {
    name: "JSON",
    keywords: &["true", "false", "null"],
    types: &[],
    line_comments: &[],
    block_comment: None,
    quotes: &['"'],
    capital_types: false,
};

static MARKUP: Lang = Lang {
    name: "Markup",
    keywords: &[],
    types: &[],
    line_comments: &[],
    block_comment: Some(("<!--", "-->")),
    quotes: &['"', '\''],
    capital_types: false,
};

static CSS: Lang = Lang {
    name: "CSS",
    keywords: &["@media", "@import", "@keyframes", "!important", "@font-face", "@supports"],
    types: &[],
    line_comments: &[],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    capital_types: false,
};

static SQL: Lang = Lang {
    name: "SQL",
    keywords: &[
        "select",
        "from",
        "where",
        "insert",
        "into",
        "values",
        "update",
        "set",
        "delete",
        "create",
        "table",
        "drop",
        "alter",
        "add",
        "join",
        "left",
        "right",
        "inner",
        "outer",
        "on",
        "group",
        "by",
        "order",
        "having",
        "limit",
        "and",
        "or",
        "not",
        "null",
        "as",
        "distinct",
        "primary",
        "key",
        "foreign",
        "references",
        "index",
        "SELECT",
        "FROM",
        "WHERE",
        "INSERT",
        "INTO",
        "VALUES",
        "UPDATE",
        "SET",
        "DELETE",
        "CREATE",
        "TABLE",
        "DROP",
        "ALTER",
        "JOIN",
        "ON",
        "GROUP",
        "BY",
        "ORDER",
        "AND",
        "OR",
        "NOT",
        "NULL",
        "AS",
    ],
    types: &["int", "integer", "text", "varchar", "boolean", "date", "timestamp", "INT", "INTEGER", "TEXT", "VARCHAR", "BOOLEAN"],
    line_comments: &["--"],
    block_comment: Some(("/*", "*/")),
    quotes: &['\'', '"'],
    capital_types: false,
};

/// The language of a file, by its extension (or name).
pub fn language(path: &str) -> Option<&'static Lang> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "Makefile" | "Dockerfile" | ".bashrc" | ".zshrc" | ".profile") {
        return Some(&SHELL);
    }
    if matches!(name, ".gitignore" | ".gitattributes" | ".gitmodules" | ".editorconfig") {
        return Some(&CONFIG);
    }
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => &RUST,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "m" | "mm" => &C_FAMILY,
        "java" | "kt" | "kts" | "go" | "swift" | "cs" | "scala" | "dart" | "groovy" | "gradle" => &JAVA_LIKE,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "vue" | "svelte" => &JS,
        "py" | "pyi" => &PYTHON,
        "rb" | "rake" | "gemspec" => &RUBY,
        "sh" | "bash" | "zsh" | "fish" | "ps1" => &SHELL,
        "toml" | "ini" | "cfg" | "conf" | "yaml" | "yml" | "properties" | "env" => &CONFIG,
        "json" | "jsonc" | "json5" | "lock" => &JSON,
        "html" | "htm" | "xml" | "svg" | "xaml" | "plist" | "csproj" | "md" | "markdown" => &MARKUP,
        "css" | "scss" | "sass" | "less" => &CSS,
        "sql" => &SQL,
        _ => return None,
    })
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Colour runs covering the whole line, in order.
pub fn tokens(line: &str, lang: &Lang) -> Vec<(Range<usize>, Kind)> {
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let push = |r: Range<usize>, k: Kind, out: &mut Vec<(Range<usize>, Kind)>| {
        if r.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, lk)) if *lk == k && last.end == r.start => last.end = r.end,
            _ => out.push((r, k)),
        }
    };
    let mut i = 0;
    let n = line.len();
    while i < n {
        let rest = line.get(i..).unwrap_or("");
        let Some(c) = rest.chars().next() else { break };
        if lang.line_comments.iter().any(|m| rest.starts_with(*m)) {
            push(i..n, Kind::Comment, &mut out);
            break;
        }
        if let Some((open, close)) = lang.block_comment
            && rest.starts_with(open)
        {
            let end = rest.get(open.len()..).and_then(|r| r.find(close)).map(|e| i + open.len() + e + close.len()).unwrap_or(n);
            push(i..end, Kind::Comment, &mut out);
            i = end;
            continue;
        }
        if lang.quotes.contains(&c) {
            let mut j = i + c.len_utf8();
            let mut escaped = false;
            let mut closed = false;
            for (k, ch) in line.get(j..).unwrap_or("").char_indices() {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == c {
                    j = j + k + ch.len_utf8();
                    closed = true;
                    break;
                }
            }
            let end = if closed { j } else { n };
            push(i..end, Kind::Str, &mut out);
            i = end;
            continue;
        }
        if c.is_ascii_digit() {
            let len = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')).unwrap_or(rest.len());
            push(i..i + len, Kind::Number, &mut out);
            i += len;
            continue;
        }
        if is_word(c) || ((c == '#' || c == '@' || c == '!') && rest.chars().nth(1).is_some_and(is_word)) {
            let skip = c.len_utf8();
            let len = skip + rest.get(skip..).unwrap_or("").find(|ch: char| !is_word(ch)).unwrap_or(rest.len().saturating_sub(skip));
            let word = rest.get(..len).unwrap_or("");
            let kind = if lang.keywords.contains(&word) {
                Kind::Keyword
            } else if lang.types.contains(&word) || (lang.capital_types && word.chars().next().is_some_and(char::is_uppercase)) {
                Kind::Type
            } else {
                Kind::Plain
            };
            push(i..i + len, kind, &mut out);
            i += len;
            continue;
        }
        push(i..i + c.len_utf8(), Kind::Plain, &mut out);
        i += c.len_utf8();
    }
    out
}

/// Words, runs of spaces, and single other characters: the units word highlights compare.
fn word_units(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let mut end = i + c.len_utf8();
        let class = |ch: char| {
            if is_word(ch) {
                0
            } else if ch.is_whitespace() {
                1
            } else {
                2
            }
        };
        let k = class(c);
        if k != 2 {
            while let Some(&(j, d)) = it.peek() {
                if class(d) != k {
                    break;
                }
                end = j + d.len_utf8();
                it.next();
            }
        }
        out.push(i..end);
    }
    out
}

/// Units compared at most per line pair (the comparison is quadratic).
const MAX_UNITS: usize = 600;

/// The parts of `old` and `new` that differ, by words: what was removed from the old line and
/// what was added in the new one. `None` when the lines are too long, or so different that
/// highlighting would only add noise.
pub fn word_changes(old: &str, new: &str) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    let a = word_units(old);
    let b = word_units(new);
    if a.is_empty() || b.is_empty() || a.len() > MAX_UNITS || b.len() > MAX_UNITS {
        return None;
    }
    let ta: Vec<&str> = a.iter().map(|r| old.get(r.clone()).unwrap_or("")).collect();
    let tb: Vec<&str> = b.iter().map(|r| new.get(r.clone()).unwrap_or("")).collect();
    // Longest common subsequence of units.
    let (n, m) = (ta.len(), tb.len());
    let w = m + 1;
    let mut dp = vec![0u16; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let v = if ta.get(i) == tb.get(j) {
                dp.get((i + 1) * w + j + 1).copied().unwrap_or(0).saturating_add(1)
            } else {
                dp.get((i + 1) * w + j).copied().unwrap_or(0).max(dp.get(i * w + j + 1).copied().unwrap_or(0))
            };
            if let Some(x) = dp.get_mut(i * w + j) {
                *x = v;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut del, mut add): (Vec<Range<usize>>, Vec<Range<usize>>) = (Vec::new(), Vec::new());
    let merge = |v: &mut Vec<Range<usize>>, r: Range<usize>| match v.last_mut() {
        Some(last) if last.end == r.start => last.end = r.end,
        _ => v.push(r),
    };
    let mut common = 0usize;
    while i < n || j < m {
        if i < n && j < m && ta.get(i) == tb.get(j) {
            if ta.get(i).is_some_and(|t| !t.trim().is_empty()) {
                common += 1;
            }
            i += 1;
            j += 1;
        } else if j < m && (i >= n || dp.get(i * w + j + 1).copied().unwrap_or(0) >= dp.get((i + 1) * w + j).copied().unwrap_or(0)) {
            if let Some(r) = b.get(j) {
                merge(&mut add, r.clone());
            }
            j += 1;
        } else {
            if let Some(r) = a.get(i) {
                merge(&mut del, r.clone());
            }
            i += 1;
        }
    }
    // Mostly rewritten: highlighting everything says nothing.
    let unchanged = ta.iter().filter(|t| !t.trim().is_empty()).count().max(1);
    if common * 3 < unchanged {
        return None;
    }
    Some((del, add))
}

/// The colour of a kind of token.
pub fn color(k: Kind, t: Tokens) -> Color32 {
    match k {
        Kind::Plain => t.text,
        Kind::Keyword => t.syn_keyword,
        Kind::Type => t.syn_type,
        Kind::Str => t.syn_string,
        Kind::Comment => t.syn_comment,
        Kind::Number => t.syn_number,
    }
}

/// A line laid out with syntax colours (when `lang` is given) and `marked` ranges on `mark_bg`.
pub fn line_job(text: &str, lang: Option<&Lang>, marked: &[Range<usize>], mark_bg: Color32, base: Color32, font: FontId, t: Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
    append_line(&mut job, text, lang, marked, mark_bg, base, font, t);
    job
}

/// [`line_job`], appended to `job`.
pub fn append_line(
    job: &mut LayoutJob,
    text: &str,
    lang: Option<&Lang>,
    marked: &[Range<usize>],
    mark_bg: Color32,
    base: Color32,
    font: FontId,
    t: Tokens,
) {
    let runs = match lang {
        Some(l) => tokens(text, l),
        None => vec![(0..text.len(), Kind::Plain)],
    };
    // Cut the syntax runs at the marked ranges' edges.
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for (r, _) in &runs {
        cuts.extend([r.start, r.end]);
    }
    for r in marked {
        cuts.extend([r.start.min(text.len()), r.end.min(text.len())]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    for w in cuts.windows(2) {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
        let Some(piece) = text.get(a..b) else { continue };
        if piece.is_empty() {
            continue;
        }
        let kind = runs.iter().find(|(r, _)| r.start <= a && a < r.end).map(|(_, k)| *k).unwrap_or(Kind::Plain);
        let fg = if lang.is_some() { color(kind, t) } else { base };
        let on = marked.iter().any(|r| r.start <= a && a < r.end);
        let fmt = TextFormat { font_id: font.clone(), color: fg, background: if on { mark_bg } else { Color32::TRANSPARENT }, ..Default::default() };
        job.append(piece, 0.0, fmt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_tokens() {
        let l = language("src/main.rs").unwrap();
        let toks = tokens("let x: u32 = 42; // answer \"q\"", l);
        let kinds: Vec<Kind> = toks.iter().map(|(_, k)| *k).collect();
        assert_eq!(kinds.first(), Some(&Kind::Keyword));
        assert!(kinds.contains(&Kind::Type) && kinds.contains(&Kind::Number));
        assert_eq!(kinds.last(), Some(&Kind::Comment));
        let s = tokens("let s = \"a \\\" b\"; x", l);
        assert!(s.iter().any(|(r, k)| *k == Kind::Str && r.len() == 8));
        // Runs cover the line exactly.
        let line = "fn é(a: &str) -> String { /* c */ }";
        let toks = tokens(line, l);
        assert_eq!(toks.first().map(|(r, _)| r.start), Some(0));
        assert_eq!(toks.last().map(|(r, _)| r.end), Some(line.len()));
        assert!(toks.windows(2).all(|w| w[0].0.end == w[1].0.start));
        assert!(language("x.unknown").is_none());
        assert_eq!(language("a/Makefile").map(|l| l.name), Some("Shell"));
    }

    #[test]
    fn word_level_changes() {
        let (del, add) = word_changes("let total = price * count;", "let total = price * amount;").unwrap();
        assert_eq!(del.len(), 1);
        assert_eq!(&"let total = price * count;"[del[0].clone()], "count");
        assert_eq!(&"let total = price * amount;"[add[0].clone()], "amount");
        assert!(word_changes("completely different words here", "nothing alike at all").is_none());
        assert!(word_changes("", "x").is_none());
    }

    #[test]
    fn junk_never_panics() {
        let l = language("a.rs").unwrap();
        for s in ["\"", "/*", "'", "#", "é\"é", "0x", "\u{202e}x", "\\", "/* é", "@"] {
            let _ = tokens(s, l);
            let _ = tokens(s, language("a.py").unwrap_or(l));
            let _ = word_changes(s, "é x");
            let _ = line_job(s, Some(l), &[0..100, 200..300], Color32::RED, Color32::WHITE, FontId::monospace(12.0), Tokens::DARK);
        }
    }
}

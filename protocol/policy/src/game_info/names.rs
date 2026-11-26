//! Game names as hosts list them, cleaned for a search, and the matching of a search's results.
//!
//! A host lists a game by whatever its launcher calls it: "DOOM Eternal™", "Mafia: Definitive
//! Edition", "Halo: The Master Chief Collection (PC)". [`clean`] takes off what no database
//! names a game by (marks, editions, platform tags) so the search finds it, and [`best`] takes a
//! result only when its name is the same game's, since wrong art or facts are worse than none.

/// Edition and platform words a launcher adds after a game's name, longest first so the longer
/// one is taken off whole. Each is matched as whole words at the end of the name.
const SUFFIXES: &[&str] = &[
    "game of the year edition",
    "windows 10 edition",
    "goty edition",
    "definitive edition",
    "deluxe edition",
    "complete edition",
    "ultimate edition",
    "standard edition",
    "special edition",
    "enhanced edition",
    "anniversary edition",
    "digital deluxe",
    "gold edition",
    "director's cut",
    "for windows",
    "pc edition",
    "goty",
];

/// Bracketed tags taken off wherever they are: platform and store tags, not part of a name.
const TAGS: &[&str] = &["pc", "steam", "windows", "win64", "x64", "64-bit", "32-bit", "dx11", "dx12", "vulkan", "beta", "demo"];

/// `name` without trademark marks, bracketed platform tags and edition suffixes, its spaces
/// collapsed. Its case and its punctuation otherwise stay, as a search wants them.
pub fn clean(name: &str) -> String {
    let mut text: String = name.chars().filter(|c| !matches!(c, '™' | '®' | '©' | '℠')).map(|c| if c.is_whitespace() || c == '_' { ' ' } else { c }).collect();
    for mark in ["(TM)", "(tm)", "(R)", "(r)", "(C)", "(c)"] {
        text = text.replace(mark, "");
    }
    text = without_tags(&text);
    loop {
        let trimmed = trim_separators(&text);
        let lower = trimmed.to_lowercase();
        let Some(suffix) = SUFFIXES.iter().find(|s| lower.ends_with(*s) && word_start(&lower, lower.len() - s.len())) else {
            text = trimmed.to_owned();
            break;
        };
        let cut = trimmed.len() - suffix.len();
        // The name must keep something: "GOTY" alone stays a name.
        if trim_separators(&trimmed[..cut]).is_empty() {
            text = trimmed.to_owned();
            break;
        }
        text = trimmed[..cut].to_owned();
    }
    collapse(&text)
}

/// `name` reduced to what decides whether two names are the same game: [`clean`], lower case,
/// `&` as "and", Roman numerals II to X as digits, and only letters, digits and single spaces.
pub fn normalize(name: &str) -> String {
    let cleaned = clean(name).to_lowercase().replace('&', " and ");
    let letters: String = cleaned
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c == '\'' || c == '’' {
                '\0'
            } else {
                ' '
            }
        })
        .filter(|c| *c != '\0')
        .collect();
    letters
        .split_whitespace()
        .map(|word| match word {
            "ii" => "2",
            "iii" => "3",
            "iv" => "4",
            "v" => "5",
            "vi" => "6",
            "vii" => "7",
            "viii" => "8",
            "ix" => "9",
            "x" => "10",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How alike two names are, from 0 to 1: 1 when they [`normalize`] to the same text, else the
/// share of words they have in common (Dice's coefficient on their words).
pub fn likeness(a: &str, b: &str) -> f32 {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    // "The Witcher 3" and "Witcher 3" are one game.
    let strip = |s: &str| s.strip_prefix("the ").map(str::to_owned).unwrap_or_else(|| s.to_owned());
    if strip(&a) == strip(&b) {
        return 1.0;
    }
    let words_a: Vec<&str> = a.split(' ').collect();
    let words_b: Vec<&str> = b.split(' ').collect();
    let is_number = |w: &&str| !w.is_empty() && w.bytes().all(|c| c.is_ascii_digit());
    if !words_a.iter().filter(|w| is_number(w)).eq(words_b.iter().filter(|w| is_number(w))) {
        return 0.0;
    }
    let mut left: Vec<&str> = words_b.clone();
    let mut common = 0;
    for word in &words_a {
        if let Some(at) = left.iter().position(|w| w == word) {
            left.swap_remove(at);
            common += 1;
        }
    }
    (2 * common) as f32 / (words_a.len() + words_b.len()) as f32
}

/// The least [`likeness`] a result needs to be taken as the game.
pub const MATCH_AT: f32 = 0.8;

/// The place in `candidates` of the one that is `query`'s game: the most alike at
/// [`MATCH_AT`] or above, the service's own order deciding a tie. None when none is alike enough.
pub fn best<S: AsRef<str>>(query: &str, candidates: &[S]) -> Option<usize> {
    let mut found: Option<(usize, f32)> = None;
    for (at, name) in candidates.iter().enumerate() {
        let score = likeness(query, name.as_ref());
        if score >= MATCH_AT && found.is_none_or(|(_, best)| score > best) {
            found = Some((at, score));
        }
    }
    found.map(|(at, _)| at)
}

/// The key a game's [`super::cache::Record`] is kept under: its Steam app id when the host gave
/// one (the same game on every host and under every name), else its [`normalize`]d name.
pub fn key(name: &str, steam: Option<u64>) -> String {
    match steam {
        Some(id) => format!("steam:{id}"),
        None => format!("name:{}", normalize(name)),
    }
}

/// The Steam app id of a host's game: from its game id, else from a `steam` store entry
/// (`(source, id)`).
pub fn steam_of(game_id: Option<&str>, store: Option<(&str, &str)>) -> Option<u64> {
    steam_id(game_id).or_else(|| store.filter(|(source, _)| *source == "steam").and_then(|(_, id)| id.trim().parse().ok()).filter(|id| *id > 0))
}

/// The Steam app id in a host's game id, as Rubylight's native API gives it (`steam:<id>`).
pub fn steam_id(game_id: Option<&str>) -> Option<u64> {
    let id = game_id?.trim().strip_prefix("steam:")?;
    id.parse().ok().filter(|id| *id > 0)
}

fn without_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(['(', '[']) {
        let close_char = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(len) = rest[open + 1..].find(close_char) else { break };
        let inner = rest[open + 1..open + 1 + len].trim().to_lowercase();
        out.push_str(&rest[..open]);
        if !TAGS.contains(&inner.as_str()) {
            out.push_str(&rest[open..open + len + 2]);
        }
        rest = &rest[open + len + 2..];
    }
    out.push_str(rest);
    out
}

fn trim_separators(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '–' | '—' | ':' | ',' | '|'))
}

fn word_start(text: &str, at: usize) -> bool {
    at == 0 || text[..at].chars().next_back().is_some_and(|c| !c.is_alphanumeric())
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` for a URL's path segment: unreserved characters as they are, every other byte as `%XX`.
pub fn path_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

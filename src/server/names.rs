//! Names players choose for the boards: usernames and Dojo solvers. Some words aren't welcome
//! there, others look official ([`blocked_names.txt`](blocked_names.txt) has both).
//!
//! A list can't catch everything, and catching too much turns away innocent names (the
//! Scunthorpe problem), so what slips through is renamed by hand on the admin dashboard.
//!
//! A name is folded first: lower case, Latin accents dropped, digits read as letters where they
//! stand for one (`sh1t`). Then it is split into words, at anything but letters and digits and
//! where a lowercase letter meets an uppercase one (`BigAss`: big, ass). Letters typed one by one
//! with separators between them (`f_u_c_k`) count as one word, and long runs of one letter are
//! shortened (`fuuuuck`).

use std::sync::LazyLock;

const LIST: &str = include_str!("blocked_names.txt");

/// The lists, folded like the names they are matched against.
#[derive(Debug, Default)]
struct Lists {
    /// Anywhere in a word.
    anywhere: Vec<String>,
    /// As a word of its own.
    words: Vec<String>,
    /// As a word of its own, usernames only.
    reserved_words: Vec<String>,
    /// As the whole username.
    reserved_names: Vec<String>,
}

static LISTS: LazyLock<Lists> = LazyLock::new(|| parse(LIST));

fn parse(text: &str) -> Lists {
    let mut lists = Lists::default();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (kind, word) = line.split_at(1);
        let word: String = word.trim().chars().flat_map(fold).collect();
        match kind {
            "*" => lists.anywhere.push(word),
            "=" => lists.words.push(word),
            "!" => lists.reserved_words.push(word),
            "^" => lists.reserved_names.push(word),
            _ => panic!("blocked_names.txt: unknown kind of entry: {line}"),
        }
    }
    lists
}

/// The word of the list a name matches, `None` if it is fine.
pub fn blocked(name: &str) -> Option<&'static str> {
    Name::new(name).matching(&LISTS)
}

/// The official looking word a username has, `None` if it has none.
pub fn reserved(username: &str) -> Option<&'static str> {
    let name = Name::new(username);
    let lists = &*LISTS;
    name.word_in(&lists.reserved_words).or_else(|| {
        lists
            .reserved_names
            .iter()
            .find(|w| **w == name.letters)
            .map(String::as_str)
    })
}

/// Lower case without accents. `ß` becomes `ss`, other letters stay as they are.
fn fold(c: char) -> impl Iterator<Item = char> {
    let mut lower = c.to_lowercase();
    let first = lower.next().unwrap_or(c);
    let (a, b) = match first {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => ('a', None),
        'ç' | 'ć' | 'č' => ('c', None),
        'ď' | 'đ' => ('d', None),
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => ('e', None),
        'ğ' => ('g', None),
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => ('i', None),
        'ł' => ('l', None),
        'ñ' | 'ń' | 'ň' => ('n', None),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => ('o', None),
        'ř' => ('r', None),
        'ś' | 'š' | 'ş' | 'ș' => ('s', None),
        'ß' => ('s', Some('s')),
        'ť' | 'ț' => ('t', None),
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => ('u', None),
        'ý' | 'ÿ' => ('y', None),
        'ź' | 'ż' | 'ž' => ('z', None),
        other => (other, lower.next()),
    };
    std::iter::once(a).chain(b)
}

/// The letter a digit or sign stands for in leetspeak.
const fn leet(c: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' | '!' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        '8' => 'b',
        '9' => 'g',
        _ => return None,
    })
}

/// Part of a word: letters, digits, and the letters of other scripts (with their marks, which
/// aren't all alphabetic, a Thai tone mark isn't).
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || (!c.is_ascii() && !c.is_whitespace())
}

/// A name, split into what the lists are matched against.
#[derive(Debug)]
struct Name {
    /// Each part between separators, folded: digits dropped and digits read as letters.
    parts: Vec<String>,
    /// The words: the parts split where a lowercase letter meets an uppercase one, and between
    /// letters and digits; also with the digits read as letters.
    words: Vec<String>,
    /// All letters, without digits and separators: `Player_17` is `player`.
    letters: String,
}

impl Name {
    fn new(name: &str) -> Self {
        // Leetspeak signs are part of a word, other signs separate words
        let raw: Vec<Vec<char>> = name
            .split(|c: char| !(word_char(c) || leet(c).is_some()))
            .filter(|p| !p.is_empty())
            .map(|p| p.chars().collect())
            .collect();
        let mut parts = Vec::new();
        let mut words = Vec::new();
        for part in &raw {
            let letters: String = part
                .iter()
                .filter(|c| word_char(**c) && !c.is_ascii_digit())
                .flat_map(|&c| fold(c))
                .collect();
            let read: String = part
                .iter()
                .flat_map(|&c| leet(c).map_or_else(|| fold(c).collect(), |l| vec![l]))
                .collect();
            for humps in camel_humps(part) {
                // Split between letters and digits, signs dropped
                let mut word = String::new();
                for (i, &c) in humps.iter().enumerate() {
                    let digit = c.is_ascii_digit();
                    if i > 0 && digit != humps[i - 1].is_ascii_digit() {
                        words.push(std::mem::take(&mut word));
                    }
                    if word_char(c) && !digit {
                        word.extend(fold(c));
                    }
                }
                words.push(word);
                words.push(
                    humps
                        .iter()
                        .flat_map(|&c| leet(c).map_or_else(|| fold(c).collect(), |l| vec![l]))
                        .collect(),
                );
            }
            parts.push(letters);
            parts.push(read);
        }
        // Letters typed one by one with separators between them: f_u_c_k
        for run in raw.chunk_by(|a, b| a.len() == 1 && b.len() == 1) {
            if run.len() > 1 && run[0].len() == 1 {
                let joined: String = run
                    .iter()
                    .map(|p| leet(p[0]).unwrap_or(p[0]))
                    .flat_map(fold)
                    .collect();
                words.push(joined.clone());
                parts.push(joined);
            }
        }
        parts.retain(|p| !p.is_empty());
        words.retain(|w| !w.is_empty());
        Self {
            parts: with_squeezed(parts),
            words: with_squeezed(words),
            letters: name
                .chars()
                .filter(|&c| word_char(c) && !c.is_ascii_digit())
                .flat_map(fold)
                .collect(),
        }
    }

    /// The first entry of the lists for every name.
    fn matching(&self, lists: &'static Lists) -> Option<&'static str> {
        lists
            .anywhere
            .iter()
            .find(|w| self.parts.iter().any(|p| p.contains(w.as_str())))
            .map(String::as_str)
            .or_else(|| self.word_in(&lists.words))
    }

    /// The entry of `list` that is one of the words, also with an -s or -es.
    fn word_in(&self, list: &'static [String]) -> Option<&'static str> {
        list.iter()
            .find(|entry| {
                self.words.iter().any(|w| {
                    w.strip_prefix(entry.as_str())
                        .is_some_and(|rest| matches!(rest, "" | "s" | "es"))
                })
            })
            .map(String::as_str)
    }
}

/// The humps of a camel case word: a new one where a lowercase letter meets an uppercase one.
fn camel_humps(part: &[char]) -> Vec<Vec<char>> {
    let mut humps = vec![Vec::new()];
    for (i, &c) in part.iter().enumerate() {
        if i > 0 && part[i - 1].is_lowercase() && c.is_uppercase() {
            humps.push(Vec::new());
        }
        if let Some(hump) = humps.last_mut() {
            hump.push(c);
        }
    }
    humps
}

/// The texts, and the same with runs of three or more of a letter shortened to one and to two
/// (fuuuck, asssshole). Double letters stay: plenty of innocent names have them.
fn with_squeezed(texts: Vec<String>) -> Vec<String> {
    let squeezed: Vec<String> = texts
        .iter()
        .flat_map(|t| {
            [squeeze(t, 1), squeeze(t, 2)]
                .into_iter()
                .filter(move |s| s != t)
        })
        .collect();
    texts.into_iter().chain(squeezed).collect()
}

fn squeeze(text: &str, keep: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for run in chars.chunk_by(|a, b| a == b) {
        let n = if run.len() >= 3 { keep } else { run.len() };
        out.extend(&run[..n]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_reads() {
        let lists = &*LISTS;
        for (kind, list) in [
            ("anywhere", &lists.anywhere),
            ("words", &lists.words),
            ("reserved words", &lists.reserved_words),
            ("reserved names", &lists.reserved_names),
        ] {
            assert!(list.len() >= 5, "{kind}: {list:?}");
            assert!(
                list.iter().all(|w| !w.is_empty() && !w.contains(' ')),
                "{kind}"
            );
        }
        // Every entry catches itself, typed as it is in the list
        for line in LIST
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let (kind, word) = line.split_at(1);
            let word = word.trim();
            let caught = match kind {
                "*" | "=" => blocked(word),
                _ => reserved(word),
            };
            assert!(caught.is_some(), "{line}");
        }
    }

    #[test]
    fn blocked_names() {
        for (name, word) in [
            ("fuck", "fuck"),
            ("FUCK", "fuck"),
            ("MotherFucker", "fuck"),
            ("f_u_c_k", "fuck"),
            ("f-u-c-k-you", "fuck"),
            ("fuuuuuck", "fuck"),
            ("Big_Ass", "ass"),
            ("BigAss", "ass"),
            ("ass99", "ass"),
            ("a55", "ass"),
            ("asses", "ass"),
            ("sh1t", "shit"),
            ("c0ck_42", "cock"),
            ("asssshole", "asshole"),
            ("Scheiße", "scheisse"),
            ("kurwa_mac", "kurwa"),
            ("Hijo-de-Puta", "puta"),
            ("Hitler88", "hitler"),
            ("SiegHeil", "siegheil"),
            ("orospu_cocugu", "orospu"),
            // Dojo names can be in any script
            ("хуйня solver", "хуй"),
            ("傻逼 solver", "傻逼"),
            ("Ça c'est de la merde", "merde"),
        ] {
            assert_eq!(blocked(name), Some(word), "{name}");
        }
    }

    /// The Scunthorpe problem: innocent names with a blocked word inside.
    #[test]
    fn innocent_names_pass() {
        for name in [
            "Scunthorpe",
            "Penistone",
            "therapist",
            "classic",
            "Classic_Gamer",
            "assassin",
            "bassist",
            "Sussex",
            "Shelly",
            "Hellen",
            "cucumber",
            "Cumbria",
            "peacock",
            "Hancock",
            "cockpit",
            "Cockburn",
            "Dickens",
            "grape",
            "drape_master",
            "computer",
            "computadora",
            "technique",
            "Monique",
            "Barsch",
            "fickle",
            "shiitake",
            "Matsushita",
            "Nazir",
            "Arsenal",
            "Analyst",
            "Peanut_Tea",
            "mississippi",
            "Whittler",
            "Hashi_Fan",
            "hashi_builder",
            "Admiral",
            "Modern_Bridges",
            "noel",
            "Bridge_Builder-99",
            "Sleepy Tanuki 27",
            "CDCL with restarts",
            "MiniSat-ish",
            "橋をかけろ",
        ] {
            assert_eq!(blocked(name), None, "{name}");
            assert_eq!(reserved(name), None, "{name}");
        }
    }

    #[test]
    fn official_looking_usernames() {
        for (name, word) in [
            ("admin", "admin"),
            ("Hashi_Admin", "admin"),
            ("HashiAdmin", "admin"),
            ("official-hashi", "official"),
            ("Moderator7", "moderator"),
            ("hashi", "hashi"),
            ("Hashi_2", "hashi"),
            ("Player_17", "player"),
            ("guest", "guest"),
        ] {
            assert_eq!(reserved(name), Some(word), "{name}");
            // Not a bad word, just official looking
            assert_eq!(blocked(name), None, "{name}");
        }
        assert_eq!(reserved("Bridge_Player"), None);
    }
}

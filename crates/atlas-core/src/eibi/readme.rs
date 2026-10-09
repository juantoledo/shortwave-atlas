//! EiBi's README.TXT, section D: language, country, target-area and transmitter-site codes.

use std::collections::{BTreeMap, HashMap};

use super::fields::parse_coord;

#[derive(Clone, Debug, PartialEq)]
pub struct Lang {
    /// English name, without the speaker counts and regions.
    pub name: String,
    /// ISO 639-3 code, when the README gives one.
    pub iso: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadmeSite {
    pub name: String,
    /// `(lat, lon)`, `None` when the README has no coordinates ("Unknown site").
    pub pos: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readme {
    pub langs: BTreeMap<String, Lang>,
    pub countries: BTreeMap<String, String>,
    pub targets: BTreeMap<String, String>,
    /// `(country, code)`; code `""` is the country's default (uncoded) site.
    pub sites: HashMap<(String, String), ReadmeSite>,
    /// Every country's sites with coordinates, in README order (the fallback for rows whose
    /// site is unknown).
    pub first_site: HashMap<String, (f64, f64)>,
}

/// `II) Country codes.`: a roman-numbered title line.
fn is_title(line: &str) -> bool {
    let t = line.trim();
    let roman = t.split_once(") ").map(|(n, _)| n).unwrap_or("");
    !roman.is_empty() && roman.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) && t.ends_with('.')
}

/// Lines of the section titled `title` (the last line that is exactly the title, trimmed,
/// since the overview repeats the names), up to the next title.
fn section_lines<'a>(lines: &[&'a str], title: &str) -> Vec<&'a str> {
    let Some(start) = lines.iter().rposition(|l| l.trim() == title) else { return Vec::new() };
    lines[start + 1..].iter().take_while(|l| !is_title(l)).copied().collect()
}

/// `   CODE  rest`: a code at column 3 followed by two or more spaces.
fn code_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("   ")?;
    if rest.starts_with(' ') {
        return None;
    }
    let end = rest.find(' ')?;
    let (code, tail) = rest.split_at(end);
    tail.starts_with("  ").then(|| (code, tail.trim()))
}

/// `Arabic (300m)  [arb]` -> name `Arabic`, iso `arb`; `Abkhaz: Georgia (0.1m)` -> `Abkhaz`.
fn lang_entry(text: &str) -> Lang {
    let mut t = text.trim();
    let mut iso = None;
    if let Some(open) = t.rfind('[') {
        let code = t[open + 1..].trim_end_matches(']');
        if t.ends_with(']') && code.len() == 3 && code.bytes().all(|b| b.is_ascii_lowercase()) {
            iso = Some(code.to_string());
            t = t[..open].trim_end();
        }
    }
    if let Some((name, _)) = t.split_once(':') {
        t = name;
    }
    let mut name = t.trim();
    while name.ends_with(')') {
        match name.rfind('(') {
            Some(i) if i > 0 => name = name[..i].trim_end(),
            _ => break,
        }
    }
    Lang { name: name.to_string(), iso }
}

/// `xx-Name 12N34-056E07 ...` or a default site `Name 12N34-056E07`.
fn site_entry(body: &str) -> (String, ReadmeSite) {
    let b = body.trim();
    let (code, text) = match b.split_once('-') {
        Some((c, t)) if (1..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphanumeric()) => (c, t),
        _ => ("", b),
    };
    let coord = parse_coord(text);
    let name_end = coord.map_or(text.len(), |c| c.2);
    let mut name = text[..name_end].trim();
    name = name.strip_suffix(" except:").unwrap_or(name);
    name = name.trim_end_matches([',', ';', ':', ' ', '-']);
    (code.to_string(), ReadmeSite { name: name.to_string(), pos: coord.map(|c| (c.0, c.1)) })
}

pub fn parse_readme(text: &str) -> Readme {
    let lines: Vec<&str> = text.lines().collect();
    let mut r = Readme::default();

    for l in section_lines(&lines, "I) Language codes.") {
        if let Some((code, rest)) = code_line(l) {
            r.langs.entry(code.to_string()).or_insert_with(|| lang_entry(rest));
        }
    }
    for l in section_lines(&lines, "II) Country codes.") {
        if let Some((code, rest)) = code_line(l) {
            r.countries.entry(code.to_string()).or_insert_with(|| rest.trim_end_matches(" *").trim().to_string());
        }
    }
    for l in section_lines(&lines, "III) Target-area codes.") {
        if let Some((code, rest)) = l.trim().split_once(" - ") {
            let code = code.trim();
            if !code.contains('.') && !code.is_empty() && !code.contains(' ') {
                r.targets.entry(code.to_string()).or_insert_with(|| rest.trim().to_string());
            }
        }
    }

    let mut country: Option<String> = None;
    for l in section_lines(&lines, "IV) Transmitter site codes.") {
        let body = if let Some((itu, body)) =
            l.strip_prefix("   ").and_then(|t| t.split_once(':')).filter(|(itu, _)| {
                (1..=3).contains(&itu.len()) && itu.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            }) {
            country = Some(itu.to_string());
            body
        } else if l.starts_with("      ") {
            l
        } else {
            // prose, or a line at column 0: not a site
            if !l.starts_with(' ') && !l.trim().is_empty() {
                country = None;
            }
            continue;
        };
        let (Some(itu), false) = (&country, body.trim().is_empty()) else { continue };
        let (code, site) = site_entry(body);
        if let Some(pos) = site.pos {
            r.first_site.entry(itu.clone()).or_insert(pos);
        }
        let key = (itu.clone(), code);
        match r.sites.get(&key) {
            Some(old) if old.pos.is_some() || site.pos.is_none() => {}
            _ => {
                r.sites.insert(key, site);
            }
        }
    }
    r
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A trimmed README.TXT with the real layout (overview titles repeat the section names).
    pub const README: &str = "\
D) Codes used.
   I)   Language codes.
   II)  Country codes.
   III) Target-area codes.
   IV)  Transmitter-site codes.

   I) Language codes.

   Numbers are number of speakers. 4m = 4 millions.
   On the right in [brackets] the ISO 639-3 (SIL) language code for reference.

   -CW   Morse Station
   -MX   Music
   A     Arabic (300m)                                                       [arb]
   AB    Abkhaz: Georgia-Abkhazia (0.1m)                                     [abk]
   AFG   Pashto and Dari (main Afghan languages, see there)
   E     English (1.5b)                                                      [eng]
   K     Korean (78m)                                                        [kor]
   M     Mandarin (Standard Chinese / Beijing dialect) (1.1b)                [cmn]
   Q     Quechua: Peru (3m)                                                  [que]
   S     Spanish (500m)                                                      [spa]
         Zomi-Chin: see Chin-Zomi (C-Z)

   II) Country codes.
   Countries are referred to by their ITU code (cf. itu.int)
   Asterisks (*) denote non-official abbreviations

   AFS  South Africa
   BUL  Bulgaria
   CHN  China (People's Republic)
   CLA  Clandestine stations *
   G    United Kingdom of Great Britain and Northern Ireland
   KOR  Korea, South (Republic)
   TWN  Taiwan *
   USA  United States of America

   III) Target-area codes.
   Af  - Africa
   C.. - Central ..
   CAm - Central America
   Eu  - Europe (often including North Africa/Middle East)
   FE  - Far East

   IV) Transmitter site codes.
   One-letter or two-letter codes for different transmitter sites within one country.
   Example: A BBC broadcast, relayed by the transmitters in Samara, Russia, would be designated as \"/RUS-s\".

   AFS: Meyerton 26S35-28E08 except:
        ct-Cape Town 33S41-18E42
        j-Johannesburg 26S07'40\"-28E12'20\"
   BUL: k-Kostinbrod 42N49-23E13
        p-Plovdiv-Padarsko 42N23-24E52
        s-Sofia-Kostinbrod 42N49-23E13
   CHN: ka-Kashi (Kashgar) (Xinjiang) 39N21-75E46
        xx-Unknown site
   KOR: g-Goyang / Gyeonggi-do 37N36-126E51
        k-Kimjae 35N50-126E50
   TWN: Paochung 23N43-120E18
   USA: a-Andrews AFB 38N48'39\"-76W52'01\"
        xx-Unknown site
US Air Force Messages: Sites are ALS-e, ASC
";

    #[test]
    fn languages() {
        let r = parse_readme(README);
        assert_eq!(r.langs["A"], Lang { name: "Arabic".into(), iso: Some("arb".into()) });
        assert_eq!(r.langs["AB"].name, "Abkhaz");
        assert_eq!(r.langs["AFG"], Lang { name: "Pashto and Dari".into(), iso: None });
        assert_eq!(r.langs["M"].name, "Mandarin");
        assert_eq!(r.langs["-MX"].name, "Music");
        assert!(!r.langs.contains_key("Numbers") && !r.langs.contains_key("On"));
        assert_eq!(r.langs.len(), 10);
    }

    #[test]
    fn countries_and_targets() {
        let r = parse_readme(README);
        assert_eq!(r.countries["CLA"], "Clandestine stations");
        assert_eq!(r.countries["G"], "United Kingdom of Great Britain and Northern Ireland");
        assert_eq!(r.countries.len(), 8);
        assert_eq!(r.targets["CAm"], "Central America");
        assert_eq!(r.targets["Eu"], "Europe (often including North Africa/Middle East)");
        assert!(!r.targets.contains_key("C.."));
    }

    #[test]
    fn sites() {
        let r = parse_readme(README);
        let site = |itu: &str, code: &str| r.sites.get(&(itu.to_string(), code.to_string())).cloned();
        assert_eq!(site("AFS", ""), Some(ReadmeSite { name: "Meyerton".into(), pos: Some((-26.5833, 28.1333)) }));
        assert_eq!(site("AFS", "j").unwrap().name, "Johannesburg");
        assert_eq!(site("CHN", "ka").unwrap().name, "Kashi (Kashgar) (Xinjiang)");
        assert_eq!(site("CHN", "xx"), Some(ReadmeSite { name: "Unknown site".into(), pos: None }));
        assert_eq!(site("TWN", ""), Some(ReadmeSite { name: "Paochung".into(), pos: Some((23.7167, 120.3)) }));
        assert_eq!(r.first_site["BUL"], (42.8167, 23.2167));
        assert_eq!(r.first_site["USA"], (38.8108, -76.8669));
        assert!(!r.first_site.contains_key("CLA"));
    }
}

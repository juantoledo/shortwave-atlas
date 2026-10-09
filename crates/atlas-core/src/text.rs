//! Text helpers for the EiBi files: decoding and accent-folding for search.

/// cp1252 code points for bytes 0x80..=0x9F (`None` = undefined in cp1252).
#[rustfmt::skip]
const CP1252_HIGH: [Option<char>; 32] = [
    Some('€'), None, Some('‚'), Some('ƒ'), Some('„'), Some('…'), Some('†'), Some('‡'),
    Some('ˆ'), Some('‰'), Some('Š'), Some('‹'), Some('Œ'), None, Some('Ž'), None,
    None, Some('‘'), Some('’'), Some('“'), Some('”'), Some('•'), Some('–'), Some('—'),
    Some('˜'), Some('™'), Some('š'), Some('›'), Some('œ'), None, Some('ž'), Some('Ÿ'),
];

fn cp1252(bytes: &[u8]) -> Option<String> {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
            _ => Some(char::from(b)),
        })
        .collect()
}

/// Decode an EiBi file: UTF-8 if valid, else cp1252, else Latin-1 (which never fails).
/// Line endings become `\n`.
pub fn decode_eibi(bytes: &[u8]) -> String {
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => cp1252(bytes).unwrap_or_else(|| bytes.iter().map(|&b| char::from(b)).collect()),
    };
    let s = s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s);
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Lowercase without accents, for matching "Alemán" with "aleman".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'æ' => out.push_str("ae"),
            'ç' | 'ć' | 'č' => out.push('c'),
            'ď' | 'đ' | 'ð' => out.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'ğ' => out.push('g'),
            'ì' | 'í' | 'î' | 'ï' | 'ı' | 'ī' => out.push('i'),
            'ł' | 'ľ' => out.push('l'),
            'ñ' | 'ń' | 'ň' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => out.push('o'),
            'œ' => out.push_str("oe"),
            'ř' => out.push('r'),
            'ś' | 'š' | 'ş' | 'ș' => out.push('s'),
            'ß' => out.push_str("ss"),
            'ť' | 'ţ' | 'ț' => out.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            'þ' => out.push_str("th"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_kept() {
        assert_eq!(decode_eibi("Rádio\r\nx".as_bytes()), "Rádio\nx");
    }

    #[test]
    fn latin1_bytes_decode() {
        // "Rádio" in Latin-1, CR line ending
        assert_eq!(decode_eibi(b"R\xe1dio\rx"), "Rádio\nx");
    }

    #[test]
    fn cp1252_quotes_decode() {
        assert_eq!(decode_eibi(b"\x93a\x94 \x80"), "“a” €");
    }

    #[test]
    fn undefined_cp1252_falls_back_to_latin1() {
        assert_eq!(decode_eibi(b"\x81\xe9"), "\u{81}é");
    }

    #[test]
    fn bom_is_dropped() {
        assert_eq!(decode_eibi("\u{feff}kHz".as_bytes()), "kHz");
    }

    #[test]
    fn fold_strips_case_and_accents() {
        assert_eq!(fold("Alemán"), "aleman");
        assert_eq!(fold("Bjørnøya São Tomé"), "bjornoya sao tome");
        assert_eq!(fold("Straße"), "strasse");
    }
}

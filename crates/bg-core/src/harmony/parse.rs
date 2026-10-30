//! Roman-numeral progressions and the library of common progressions.

use super::{ChordSpec, Scale};

/// Common progressions, as roman numerals relative to the selected scale. Case is
/// cosmetic and there are no quality markers: qualities always follow the scale, so
/// every entry works in every mode ("7" = the diatonic seventh chord).
pub const LIBRARY: [(&str, &str, &str); 12] = [
    ("pop", "Pop (I–V–vi–IV)", "I V vi IV"),
    ("doo_wop", "Doo-wop (I–vi–IV–V)", "I vi IV V"),
    ("ii_v_i", "Jazz ii–V–I", "ii7 V7 I7 I7"),
    ("rhythm", "Rhythm changes (I–vi–ii–V)", "I7 vi7 ii7 V7"),
    ("iii_vi_ii_v", "iii–vi–ii–V–I", "iii7 vi7 ii7 V7 I7 I7"),
    ("classic", "Classic cadence (I–IV–V–I)", "I IV V I"),
    ("royal_road", "Royal road (IV–V–iii–vi)", "IV7 V7 iii7 vi7"),
    ("pachelbel", "Pachelbel (I–V–vi–iii–IV–I–IV–V)", "I V vi iii IV I IV V"),
    ("blues", "Diatonic blues (I–IV–I–V–IV–I)", "I I I I IV IV I I V IV I V"),
    ("minor_plagal", "Minor (i–iv–v–i)", "i iv v i"),
    ("minor_epic", "Minor epic (i–VI–III–VII)", "i VI III VII"),
    ("circle", "Circle of fifths (vi–ii–V–I)", "vi7 ii7 V7 I7"),
];

pub fn library(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    LIBRARY.iter().copied().find(|e| e.0 == id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte range of the offending token.
    pub start: usize,
    pub end: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at {})", self.message, self.start + 1)
    }
}

/// Parses e.g. `"ii7 V7 Imaj7 vi"`. Tokens are separated by spaces, commas, `-`, `–` or
/// `|`. Each token is a numeral (I–VII, any case), an optional quality marker
/// (`m`, `maj`, `°`/`o`/`dim`, `ø`) checked against `scale`, and an optional size
/// (`7`, `9`, `11`, `13`).
pub fn parse_progression(text: &str, scale: &Scale) -> Result<Vec<ChordSpec>, ParseError> {
    let mut out = Vec::new();
    let mut i = 0;
    let is_sep = |c: char| c.is_whitespace() || matches!(c, ',' | '-' | '–' | '|');
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap();
        if is_sep(c) {
            i += c.len_utf8();
            continue;
        }
        let len = rest.find(is_sep).unwrap_or(rest.len());
        out.push(parse_token(&text[i..i + len], scale).map_err(|message| ParseError { start: i, end: i + len, message })?);
        i += len;
    }
    if out.is_empty() {
        return Err(ParseError { start: 0, end: text.len(), message: "enter at least one chord, e.g. \"ii7 V7 I\"".into() });
    }
    Ok(out)
}

fn parse_token(tok: &str, scale: &Scale) -> Result<ChordSpec, String> {
    const NUMERALS: [&str; 7] = ["vii", "iii", "vi", "iv", "ii", "v", "i"];
    const DEGREES: [usize; 7] = [6, 2, 5, 3, 1, 4, 0];
    let lower = tok.to_lowercase();
    let (n, degree) = NUMERALS
        .iter()
        .zip(DEGREES)
        .find(|(n, _)| lower.starts_with(**n))
        .map(|(n, d)| (n.len(), d))
        .ok_or_else(|| format!("\"{tok}\" doesn't start with a numeral I–VII"))?;
    let mut rest = &tok[n..];
    let mut quality = None;
    for q in ["maj", "dim", "m", "°", "o", "ø", "M"] {
        if let Some(r) = rest.strip_prefix(q) {
            quality = Some(q);
            rest = r;
            break;
        }
    }
    let size = match rest {
        "" => 3,
        "7" => 4,
        "9" => 5,
        "11" => 6,
        "13" => 7,
        _ => return Err(format!("unknown chord size \"{rest}\" in \"{tok}\" (use 7, 9, 11 or 13)")),
    };
    // Qualities are diatonic; a marker must agree with what the scale gives.
    let chord = scale.realize(ChordSpec { degree, size: size.max(4) });
    let third = chord.interval(chord.third());
    let fifth = chord.interval(chord.triad[2]);
    let seventh = chord.seventh.map(|s| chord.interval(s));
    let ok = match quality {
        None => true,
        Some("m") => third == 3 && fifth == 7,
        Some("maj" | "M") => third == 4 && (size == 3 || seventh == Some(11)),
        Some("dim" | "°" | "o") => third == 3 && fifth == 6,
        Some("ø") => third == 3 && fifth == 6 && seventh == Some(10),
        _ => true,
    };
    if !ok {
        let actual = scale.realize(ChordSpec { degree, size }).roman();
        return Err(format!("\"{tok}\" isn't diatonic in {}: that degree is {actual}", scale.name()));
    }
    Ok(ChordSpec { degree, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::Mode;

    const C: Scale = Scale { root: 0, mode: Mode::Ionian };

    #[test]
    fn parses_numerals_and_sizes() {
        let p = parse_progression("ii7 V7 | Imaj7, vi - IV9 iii13", &C).unwrap();
        assert_eq!(
            p,
            vec![
                ChordSpec { degree: 1, size: 4 },
                ChordSpec { degree: 4, size: 4 },
                ChordSpec { degree: 0, size: 4 },
                ChordSpec { degree: 5, size: 3 },
                ChordSpec { degree: 3, size: 5 },
                ChordSpec { degree: 2, size: 7 },
            ]
        );
        assert_eq!(parse_progression("viiø7 vii° iim7", &C).unwrap().len(), 3);
    }

    #[test]
    fn errors_point_at_the_token() {
        let e = parse_progression("ii7 X7 I", &C).unwrap_err();
        assert_eq!((e.start, e.end), (4, 6));
        let e = parse_progression("I V7 IVm", &C).unwrap_err();
        assert_eq!(&"I V7 IVm"[e.start..e.end], "IVm");
        assert!(e.message.contains("IV"), "{}", e.message);
        let e = parse_progression("ii8", &C).unwrap_err();
        assert!(e.message.contains("size"));
        assert!(parse_progression("  ", &C).is_err());
    }

    #[test]
    fn library_parses_in_every_mode() {
        for mode in Mode::ALL {
            let scale = Scale { root: 7, mode };
            for (id, _, text) in LIBRARY {
                assert!(parse_progression(text, &scale).is_ok(), "{id} in {mode:?}");
            }
        }
    }
}

//! Text helpers for Turkish-aware search.

/// Folds text for search and uniqueness: Turkish-aware lowercase, diacritics
/// removed (ç→c, ğ→g, ı/İ→i, ö→o, ş→s, ü→u, â→a …), whitespace collapsed.
///
/// `"KÖY"`, `"Köy"` and `"koy"` all become `"koy"`.
pub fn normalize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        match ch {
            'I' | 'İ' | 'ı' | 'i' | 'î' | 'Î' => out.push('i'),
            'Ç' | 'ç' => out.push('c'),
            'Ğ' | 'ğ' => out.push('g'),
            'Ö' | 'ö' => out.push('o'),
            'Ş' | 'ş' => out.push('s'),
            'Ü' | 'ü' | 'û' | 'Û' => out.push('u'),
            'Â' | 'â' => out.push('a'),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn folds_turkish_letters() {
        assert_eq!(normalize("Köy"), "koy");
        assert_eq!(normalize("KÖY"), "koy");
        assert_eq!(normalize("İstanbul"), "istanbul");
        assert_eq!(normalize("ISPARTA"), "isparta");
        assert_eq!(normalize("Nişan 2022"), "nisan 2022");
        assert_eq!(normalize("Düğün"), "dugun");
        assert_eq!(normalize("Çağrı"), "cagri");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(normalize("  Ramazan   Bayramı \t2023 "), "ramazan bayrami 2023");
    }
}

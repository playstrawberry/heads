//! The plugin's texts in English, French, Spanish and Turkish (`lang/<code>.lang`, `key=text`
//! lines; `{0}` and `{1}` are filled in). Any other language reads English.

const FILES: [(&str, &str); 4] = [
    ("en", include_str!("../lang/en.lang")),
    ("fr", include_str!("../lang/fr.lang")),
    ("es", include_str!("../lang/es.lang")),
    ("tr", include_str!("../lang/tr.lang")),
];

/// The keys every language file has.
pub const KEYS: [&str; 4] = ["refreshed", "not_online", "status", "usage"];

/// The language of a locale written as Minecraft writes it (`fr_fr`, `FrCa`, `tr-tr`).
#[must_use]
pub fn language(locale: &str) -> &'static str {
    let code = locale.get(..2).unwrap_or("").to_ascii_lowercase();
    FILES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| *c == code)
        .unwrap_or("en")
}

fn lookup<'a>(file: &'a str, key: &str) -> Option<&'a str> {
    file.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
}

/// The text of `key` in `language`, with `args` in place of `{0}`, `{1}`, ...; English when the
/// language lacks the key, the key itself when English lacks it too.
#[must_use]
pub fn text(language: &str, key: &str, args: &[&str]) -> String {
    let file = |code: &str| FILES.iter().find(|(c, _)| *c == code).map(|(_, f)| *f);
    let raw = file(language)
        .and_then(|f| lookup(f, key))
        .or_else(|| file("en").and_then(|f| lookup(f, key)))
        .unwrap_or(key);
    let mut out = raw.to_owned();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key_with_the_same_placeholders() {
        for (code, file) in FILES {
            for key in KEYS {
                let found = lookup(file, key).unwrap_or_else(|| panic!("{code} lacks {key}"));
                let english = lookup(FILES[0].1, key).unwrap();
                for slot in ["{0}", "{1}"] {
                    assert_eq!(
                        found.contains(slot),
                        english.contains(slot),
                        "{code} {key} {slot}"
                    );
                }
            }
        }
    }

    #[test]
    fn locales_pick_their_language_and_others_read_english() {
        assert_eq!(language("fr_fr"), "fr");
        assert_eq!(language("FrCa"), "fr");
        assert_eq!(language("es_mx"), "es");
        assert_eq!(language("tr-tr"), "tr");
        assert_eq!(language("de_de"), "en");
        assert_eq!(language(""), "en");
        assert_ne!(
            text("tr", "refreshed", &["Steve"]),
            text("en", "refreshed", &["Steve"])
        );
        assert!(text("tr", "refreshed", &["Steve"]).starts_with("Steve"));
        assert!(text("en", "refreshed", &["Steve"]).contains("Steve"));
        assert_eq!(text("en", "no_such_key", &[]), "no_such_key");
    }
}

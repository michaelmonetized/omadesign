//! Knuth–Liang hyphenation. Only permissively licensed American English patterns ship.
use crate::geom::HyphenSettings;
use hyphenation::{Hyphenator, Language, Load, Standard};
use std::sync::OnceLock;

/// Character offsets, independent of UTF-8 byte width. Composer owns line limits/zone.
pub fn hyphen_points(word: &str, settings: &HyphenSettings) -> Vec<usize> {
    let count = word.chars().count();
    if count < settings.min_word_len
        || (!settings.capitalized && word.chars().next().is_some_and(char::is_uppercase))
    {
        return vec![];
    }
    static EN_US: OnceLock<Standard> = OnceLock::new();
    let dictionary = EN_US.get_or_init(|| {
        Standard::from_embedded(Language::EnglishUS).expect("bundled en-US dictionary")
    });
    // The language field retains the requested locale; en-US is the shipped fallback.
    dictionary
        .hyphenate(word)
        .breaks
        .into_iter()
        .filter(|&byte| word.is_char_boundary(byte))
        .map(|byte| word[..byte].chars().count())
        .filter(|&index| {
            index >= settings.min_before && count.saturating_sub(index) >= settings.min_after
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dictionary_and_limits() {
        let mut s = HyphenSettings::default();
        let points = hyphen_points("hyphenation", &s);
        assert!(!points.is_empty());
        assert!(points.iter().all(|i| *i >= 3 && *i <= 8));
        s.min_before = 7;
        s.min_after = 5;
        assert!(hyphen_points("hyphenation", &s).is_empty());
    }
    #[test]
    fn capitalization_is_optional() {
        let mut s = HyphenSettings::default();
        assert!(hyphen_points("Hyphenation", &s).is_empty());
        s.capitalized = true;
        assert!(!hyphen_points("Hyphenation", &s).is_empty());
    }
}

//! Nucleo fuzzy matching: smart case, Unicode normalization, word-wise; empty query keeps input order.

use nucleo::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32String,
};

/// One match, referring to the original input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub index: usize,
    /// Sorted Unicode scalar indices in the label.
    pub indices: Vec<u32>,
}

pub struct FuzzyFinder {
    labels: Vec<Utf32String>,
    matcher: Matcher,
}

impl FuzzyFinder {
    pub fn new(labels: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            labels: labels.into_iter().map(|s| Utf32String::from(s.as_ref())).collect(),
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub fn search(&mut self, query: &str) -> Vec<Match> {
        let pattern = Pattern::new(query, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
        let mut scored: Vec<_> = self
            .labels
            .iter()
            .enumerate()
            .filter_map(|(index, label)| {
                let mut indices = Vec::new();
                let score = pattern.indices(label.slice(..), &mut self.matcher, &mut indices)?;
                indices.sort_unstable();
                indices.dedup();
                Some((score, Match { index, indices }))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.index.cmp(&b.1.index)));
        scored.into_iter().map(|(_, m)| m).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indices(finder: &mut FuzzyFinder, query: &str) -> Vec<usize> {
        finder.search(query).iter().map(|m| m.index).collect()
    }

    #[test]
    fn search_with_empty_query_preserves_input_order() {
        assert_eq!(indices(&mut FuzzyFinder::new(["z", "a", "a"]), ""), [0, 1, 2]);
    }

    #[test]
    fn search_ranks_the_tighter_match_first() {
        assert_eq!(indices(&mut FuzzyFinder::new(["a long bumpy c", "abc"]), "abc")[0], 1);
    }

    #[test]
    fn search_drops_labels_that_do_not_match() {
        assert!(FuzzyFinder::new(["abc"]).search("xyz").is_empty());
    }

    #[test]
    fn search_reports_matched_character_positions() {
        assert_eq!(FuzzyFinder::new(["éclair"]).search("éc")[0].indices, [0, 1]);
    }
}

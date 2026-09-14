use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct Match {
    pub index: usize,
    pub char_indices: Vec<usize>,
}

/// Fuzzy-filters `haystacks` against `query`, best match first. An empty
/// query keeps everything, in its original order, with nothing highlighted —
/// this is what the list looks like before the user types anything.
pub fn filter_sort(haystacks: &[String], query: &str) -> Vec<Match> {
    if query.is_empty() {
        return (0..haystacks.len()).map(|index| Match { index, char_indices: Vec::new() }).collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, Match)> = haystacks
        .iter()
        .enumerate()
        .filter_map(|(index, haystack)| {
            matcher
                .fuzzy_indices(haystack, query)
                .map(|(score, char_indices)| (score, Match { index, char_indices }))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, m)| m).collect()
}

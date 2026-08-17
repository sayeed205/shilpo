/// Match result carrying match score and character positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    /// Match quality score. Higher is better.
    pub score: i64,
    /// 0-based character indices into the original target string.
    pub positions: Vec<usize>,
}

const SCORE_EXACT_MATCH: i64 = 1000;
const SCORE_FULL_CONTIGUOUS: i64 = 150;
const SCORE_PREFIX: i64 = 40;
const SCORE_WORD_BOUNDARY: i64 = 30;
const SCORE_CONSECUTIVE: i64 = 30;
const SCORE_BASE_MATCH: i64 = 10;
const SCORE_CASE_MATCH: i64 = 5;
const SCORE_GAP_OPEN: i64 = 20;
const SCORE_GAP_EXTENSION: i64 = 1;
const SCORE_LEADING_GAP_PENALTY: i64 = 2;
const MAX_LEADING_PENALTY: i64 = 25;
const SCORE_TRAILING_GAP_PENALTY: i64 = 1;
const NEG_INFINITY: i64 = -1_000_000_000;

/// Matches a query against a target string using Smith-Waterman / fzy dynamic programming.
///
/// Returns `Some(MatchResult)` containing the score and matched character positions if `query`
/// is a case-insensitive subsequence of `target`. Returns `None` if:
/// - `query` is empty
/// - `query` character count exceeds `target` character count
/// - `query` is not a subsequence of `target`
pub fn fuzzy_match(query: &str, target: &str) -> Option<MatchResult> {
    if query.is_empty() || target.is_empty() {
        return None;
    }

    let q_chars: Vec<char> = query.chars().collect();
    let t_chars: Vec<char> = target.chars().collect();

    let n = q_chars.len();
    let m = t_chars.len();

    if n > m {
        return None;
    }

    // Quick linear check: query must be a subsequence of target.
    let mut q_idx = 0;
    for &tc in &t_chars {
        if q_idx < n && chars_equal_ignore_case(q_chars[q_idx], tc) {
            q_idx += 1;
        }
    }
    if q_idx < n {
        return None;
    }

    // Precompute character bonuses for target.
    let mut bonuses = Vec::with_capacity(m);
    for j in 0..m {
        let mut bonus = 0;
        let curr = t_chars[j];
        if j == 0 {
            bonus += SCORE_PREFIX + SCORE_WORD_BOUNDARY;
        } else {
            let prev = t_chars[j - 1];
            if is_delimiter(prev) || (prev.is_lowercase() && curr.is_uppercase()) {
                bonus += SCORE_WORD_BOUNDARY;
            }
        }
        bonuses.push(bonus);
    }

    // DP state:
    // match_matrix[i * m + j]: best score for Q[0..=i] ending with Q[i] matched at T[j].
    // best_matrix[i * m + j]: best score for Q[0..=i] matched anywhere at or before T[j].
    let mut match_matrix = vec![NEG_INFINITY; n * m];
    let mut best_matrix = vec![NEG_INFINITY; n * m];

    // Base case: i = 0 (first query character)
    for j in 0..m {
        if chars_equal_ignore_case(q_chars[0], t_chars[j]) {
            let leading_penalty = ((j as i64) * SCORE_LEADING_GAP_PENALTY).min(MAX_LEADING_PENALTY);
            let mut score = SCORE_BASE_MATCH + bonuses[j] - leading_penalty;
            if q_chars[0] == t_chars[j] {
                score += SCORE_CASE_MATCH;
            }
            match_matrix[j] = score;
            best_matrix[j] = if j == 0 {
                score
            } else {
                best_matrix[j - 1].max(score)
            };
        } else {
            best_matrix[j] = if j == 0 {
                NEG_INFINITY
            } else {
                best_matrix[j - 1]
            };
        }
    }

    // Inductive step: i = 1..n
    for (i, &qc) in q_chars.iter().enumerate().take(n).skip(1) {
        let prev_row = (i - 1) * m;
        let curr_row = i * m;

        for j in 1..m {
            if chars_equal_ignore_case(qc, t_chars[j]) {
                let case_bonus = if qc == t_chars[j] {
                    SCORE_CASE_MATCH
                } else {
                    0
                };

                // Consecutive match from previous character
                let prev_match = match_matrix[prev_row + (j - 1)];
                let consecutive_score = if prev_match != NEG_INFINITY {
                    prev_match + SCORE_BASE_MATCH + SCORE_CONSECUTIVE + case_bonus
                } else {
                    NEG_INFINITY
                };

                // Match with a gap from best previous prefix match
                let mut best_gap_score = NEG_INFINITY;
                for k in 0..(j - 1) {
                    let prev_k_score = match_matrix[prev_row + k];
                    if prev_k_score != NEG_INFINITY {
                        let gap_len = (j - 1 - k) as i64;
                        let gap_penalty = SCORE_GAP_OPEN + gap_len * SCORE_GAP_EXTENSION;
                        let cand_score =
                            prev_k_score + SCORE_BASE_MATCH + bonuses[j] - gap_penalty + case_bonus;
                        if cand_score > best_gap_score {
                            best_gap_score = cand_score;
                        }
                    }
                }

                let score = consecutive_score.max(best_gap_score);
                match_matrix[curr_row + j] = score;
                best_matrix[curr_row + j] = best_matrix[curr_row + (j - 1)].max(score);
            } else {
                best_matrix[curr_row + j] = best_matrix[curr_row + (j - 1)];
            }
        }
    }

    // Find best ending index for last query character, penalizing trailing characters.
    let last_row = (n - 1) * m;
    let mut best_final_score = NEG_INFINITY;
    let mut best_last_j = 0;

    for j in (n - 1)..m {
        let score = match_matrix[last_row + j];
        if score != NEG_INFINITY {
            let trailing_penalty = (m - 1 - j) as i64 * SCORE_TRAILING_GAP_PENALTY;
            let total_score = score - trailing_penalty;
            if total_score > best_final_score {
                best_final_score = total_score;
                best_last_j = j;
            }
        }
    }

    if best_final_score <= NEG_INFINITY / 2 {
        return None;
    }

    // Backtrack to recover optimal match character positions.
    let mut positions = Vec::with_capacity(n);
    let mut curr_j = best_last_j;

    positions.push(curr_j);

    for i in (1..n).rev() {
        let prev_row = (i - 1) * m;
        let curr_row = i * m;
        let qc = q_chars[i];
        let case_bonus = if qc == t_chars[curr_j] {
            SCORE_CASE_MATCH
        } else {
            0
        };

        // Check if curr_j came from consecutive match
        let prev_match = match_matrix[prev_row + (curr_j - 1)];
        let is_consecutive = prev_match != NEG_INFINITY
            && match_matrix[curr_row + curr_j]
                == prev_match + SCORE_BASE_MATCH + SCORE_CONSECUTIVE + case_bonus;

        if is_consecutive {
            curr_j -= 1;
        } else {
            // Find k < curr_j that produced match_matrix[curr_row + curr_j]
            let mut found_k = 0;
            let target_score = match_matrix[curr_row + curr_j];
            for k in (0..curr_j.saturating_sub(1)).rev() {
                let prev_k_score = match_matrix[prev_row + k];
                if prev_k_score != NEG_INFINITY {
                    let gap_len = (curr_j - 1 - k) as i64;
                    let gap_penalty = SCORE_GAP_OPEN + gap_len * SCORE_GAP_EXTENSION;
                    if prev_k_score + SCORE_BASE_MATCH + bonuses[curr_j] - gap_penalty + case_bonus
                        == target_score
                    {
                        found_k = k;
                        break;
                    }
                }
            }
            curr_j = found_k;
        }
        positions.push(curr_j);
    }

    positions.reverse();

    // Bonus for full contiguous match
    if positions.len() == n && (positions[n - 1] - positions[0] + 1) == n {
        best_final_score += SCORE_FULL_CONTIGUOUS;
    }

    // Add exact match bonus if query matches full target length.
    if n == m {
        best_final_score += SCORE_EXACT_MATCH;
    }

    Some(MatchResult {
        score: best_final_score,
        positions,
    })
}

/// Convenience scoring function that computes match score without returning positions.
pub fn fuzzy_score(query: &str, target: &str) -> Option<i64> {
    fuzzy_match(query, target).map(|m| m.score)
}

#[inline]
fn chars_equal_ignore_case(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    a.to_lowercase().eq(b.to_lowercase())
}

#[inline]
fn is_delimiter(c: char) -> bool {
    matches!(
        c,
        ' ' | '-' | '_' | '/' | '.' | ':' | '\\' | '\t' | '@' | '#' | '|' | '(' | '['
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsequence_matching() {
        let m1 = fuzzy_match("fx", "Firefox");
        assert!(m1.is_some(), "fx should match Firefox");
        let res1 = m1.unwrap();
        assert_eq!(res1.positions, vec![0, 6]);

        let m2 = fuzzy_match("dckr", "Docker Desktop");
        assert!(m2.is_some(), "dckr should match Docker Desktop");
        let res2 = m2.unwrap();
        assert_eq!(res2.positions, vec![0, 2, 3, 5]);
    }

    #[test]
    fn test_consecutive_runs_outrank_scattered_matches() {
        let query = "fire";
        let target_consecutive = "Firefox";
        let target_scattered = "f_i_r_e_f_o_x";

        let score_consecutive = fuzzy_score(query, target_consecutive).unwrap();
        let score_scattered = fuzzy_score(query, target_scattered).unwrap();

        assert!(
            score_consecutive > score_scattered,
            "consecutive ({score_consecutive}) should outrank scattered ({score_scattered})"
        );
    }

    #[test]
    fn test_word_boundary_and_camel_case_outrank_mid_word() {
        let query = "desk";
        let word_boundary = "Docker Desktop";
        let mid_word = "Predesking Window";

        let score_boundary = fuzzy_score(query, word_boundary).unwrap();
        let score_mid = fuzzy_score(query, mid_word).unwrap();

        assert!(
            score_boundary > score_mid,
            "word boundary ({score_boundary}) should outrank mid-word ({score_mid})"
        );
    }

    #[test]
    fn test_exact_outranks_prefix_which_outranks_subsequence() {
        let query = "term";
        let exact = "term";
        let prefix = "terminal";
        let subsequence = "troubleshoot error messages";

        let score_exact = fuzzy_score(query, exact).unwrap();
        let score_prefix = fuzzy_score(query, prefix).unwrap();
        let score_subsequence = fuzzy_score(query, subsequence).unwrap();

        assert!(
            score_exact > score_prefix,
            "exact ({score_exact}) should outrank prefix ({score_prefix})"
        );
        assert!(
            score_prefix > score_subsequence,
            "prefix ({score_prefix}) should outrank subsequence ({score_subsequence})"
        );
    }

    #[test]
    fn test_returned_positions_reconstruct_query_in_order() {
        let query = "calc";
        let target = "Gnome Calculator";
        let res = fuzzy_match(query, target).unwrap();

        let t_chars: Vec<char> = target.chars().collect();
        let reconstructed: String = res.positions.iter().map(|&idx| t_chars[idx]).collect();

        assert_eq!(reconstructed.to_lowercase(), query.to_lowercase());
    }

    #[test]
    fn test_multibyte_utf8_positions_and_safety() {
        let query = "rust";
        let target = "🦀 Rust 🚀 Launch";
        let res = fuzzy_match(query, target).unwrap();

        let t_chars: Vec<char> = target.chars().collect();
        assert_eq!(t_chars.len(), 15);
        assert_eq!(res.positions, vec![2, 3, 4, 5]);

        let reconstructed: String = res.positions.iter().map(|&idx| t_chars[idx]).collect();
        assert_eq!(reconstructed.to_lowercase(), "rust");

        // Café query
        let m_cafe = fuzzy_match("noir", "Café Noir").unwrap();
        let cafe_chars: Vec<char> = "Café Noir".chars().collect();
        assert_eq!(m_cafe.positions, vec![5, 6, 7, 8]);
        let cafe_reconstructed: String = m_cafe
            .positions
            .iter()
            .map(|&idx| cafe_chars[idx])
            .collect();
        assert_eq!(cafe_reconstructed.to_lowercase(), "noir");
    }

    #[test]
    fn test_empty_too_long_and_no_match_return_none() {
        assert_eq!(fuzzy_match("", "Target"), None);
        assert_eq!(fuzzy_match("Query", ""), None);
        assert_eq!(fuzzy_match("LongerQueryThanTarget", "Short"), None);
        assert_eq!(fuzzy_match("xyz", "Firefox"), None);
    }

    #[test]
    fn test_scoring_determinism_property() {
        let query = "code";
        let target = "Visual Studio Code";

        let first = fuzzy_match(query, target).unwrap();
        for _ in 0..100 {
            let next = fuzzy_match(query, target).unwrap();
            assert_eq!(first, next);
        }
    }

    #[test]
    fn test_pathological_input_completes_bounded() {
        let query = "a".repeat(32);
        let target = "ab".repeat(128);

        let start = std::time::Instant::now();
        let res = fuzzy_match(&query, &target);
        let elapsed = start.elapsed();

        assert!(res.is_some());
        assert!(elapsed < std::time::Duration::from_secs(2));
    }

    #[test]
    fn test_golden_intra_field_ordering_term() {
        let query = "term";
        let candidates = [
            "Terminal",
            "Gnome Terminal",
            "XTerm",
            "Determined",
            "troubleshoot error messages",
        ];

        let mut scored: Vec<(&str, i64)> = candidates
            .iter()
            .map(|&c| (c, fuzzy_score(query, c).unwrap()))
            .collect();

        // Sort descending by score
        scored.sort_by_key(|b| std::cmp::Reverse(b.1));

        let ranked_titles: Vec<&str> = scored.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            ranked_titles,
            vec![
                "Terminal",
                "Gnome Terminal",
                "XTerm",
                "Determined",
                "troubleshoot error messages",
            ]
        );
    }

    #[test]
    fn test_golden_intra_field_ordering_calc() {
        let query = "calc";
        let candidates = [
            "calc",
            "Calculator",
            "Gnome Calculator",
            "LibreOffice Calc",
            "Decalcify",
            "Climatic Area Land Cover",
        ];

        let mut scored: Vec<(&str, i64)> = candidates
            .iter()
            .map(|&c| (c, fuzzy_score(query, c).unwrap()))
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.1));

        let ranked_titles: Vec<&str> = scored.iter().map(|(c, _)| *c).collect();
        assert_eq!(
            ranked_titles,
            vec![
                "calc",
                "Calculator",
                "Gnome Calculator",
                "LibreOffice Calc",
                "Decalcify",
                "Climatic Area Land Cover",
            ]
        );
    }

    #[test]
    fn test_matcher_invariants_and_properties() {
        let test_cases = [
            ("fx", "Firefox Browser"),
            ("code", "Visual Studio Code"),
            ("sh", "Shilpo Desktop Shell"),
            ("calc", "Gnome Calculator"),
            ("term", "XTerm terminal emulator"),
            ("🦀", "Hello 🦀 World 🚀"),
            ("world", "Hello 🦀 World 🚀"),
        ];

        for (q, t) in test_cases {
            let res = fuzzy_match(q, t).expect("should match");
            let t_chars: Vec<char> = t.chars().collect();

            // 1. Length invariant
            assert_eq!(res.positions.len(), q.chars().count());

            // 2. Strict monotonicity invariant
            for w in res.positions.windows(2) {
                assert!(w[0] < w[1], "positions must be strictly increasing");
            }

            // 3. Bounds invariant
            for &pos in &res.positions {
                assert!(pos < t_chars.len(), "position must be in bounds");
            }

            // 4. Character reconstruction invariant
            let reconstructed: String = res.positions.iter().map(|&idx| t_chars[idx]).collect();
            assert_eq!(
                reconstructed.to_lowercase(),
                q.to_lowercase(),
                "reconstructed string must match query"
            );
        }
    }
}

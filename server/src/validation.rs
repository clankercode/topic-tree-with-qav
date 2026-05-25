//! Shared validation primitives for client-visible inputs.
//!
//! Kept aligned with `web/src/lib/validation.ts`. The parallel test
//! suite there exercises the same edge cases — when a behaviour
//! changes here, update both sides in the same commit.

/// Count whitespace-delimited words in `input`. Empty / whitespace-only
/// strings return 0. Multiple whitespace runs (including NBSP and
/// other Unicode whitespace classes) count as a single separator.
/// Zero-width joiners (U+200D) do NOT split words.
///
/// Rust's `split_whitespace` is the canonical reference for what
/// counts as whitespace; JavaScript's `\s` character class is
/// equivalent for these cases so the client-side counter can use a
/// terse regex.
pub fn count_topic_words(input: &str) -> usize {
    input.split_whitespace().count()
}

/// Maximum allowed words in a raise-hand topic.
pub const MAX_RAISE_HAND_TOPIC_WORDS: usize = 10;

/// Maximum allowed length (chars) of a raise-hand topic.
pub const MAX_RAISE_HAND_TOPIC_LEN: usize = 80;

#[cfg(test)]
mod tests {
    use super::*;

    /// Parity cases for `web/src/lib/validation.test.ts::countTopicWords`.
    /// If either side diverges, the user-facing word-count gauge will
    /// disagree with the server's accept/reject decision.
    #[test]
    fn empty_and_whitespace_only_count_as_zero() {
        assert_eq!(count_topic_words(""), 0);
        assert_eq!(count_topic_words("   "), 0);
        assert_eq!(count_topic_words("\t\n  "), 0);
    }

    #[test]
    fn single_word_counts_as_one() {
        assert_eq!(count_topic_words("foo"), 1);
    }

    #[test]
    fn whitespace_separators_each_count_as_one_break() {
        assert_eq!(count_topic_words("foo bar"), 2);
        assert_eq!(count_topic_words("foo  bar"), 2);
        assert_eq!(count_topic_words("foo\tbar"), 2);
        assert_eq!(count_topic_words("foo\nbar"), 2);
    }

    #[test]
    fn nbsp_is_treated_as_whitespace() {
        // U+00A0 NON-BREAKING SPACE
        assert_eq!(count_topic_words("foo\u{00A0}bar"), 2);
    }

    #[test]
    fn zero_width_joiner_does_not_split_words() {
        // U+200D ZERO WIDTH JOINER — should be glued into the same word.
        assert_eq!(count_topic_words("foo\u{200D}bar"), 1);
    }

    #[test]
    fn hyphen_is_part_of_the_word() {
        assert_eq!(count_topic_words("foo-bar"), 1);
    }

    #[test]
    fn exactly_ten_words_is_allowed() {
        let s = "a b c d e f g h i j";
        assert_eq!(count_topic_words(s), MAX_RAISE_HAND_TOPIC_WORDS);
    }

    #[test]
    fn eleven_words_exceeds_limit() {
        let s = "a b c d e f g h i j k";
        assert!(count_topic_words(s) > MAX_RAISE_HAND_TOPIC_WORDS);
    }
}

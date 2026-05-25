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

/// Task #13: bounds for `ImportTopicTree`. A malicious or buggy
/// import could otherwise create thousands of nested topics and
/// overwhelm the room's in-memory state + every connected client's
/// renderer.
pub const MAX_IMPORT_TOPICS: usize = 500;
/// Maximum nesting depth for topics (root = depth 1). Enforced on import,
/// add, and move.
pub const MAX_TOPIC_DEPTH: usize = 10;
pub const MAX_IMPORT_DEPTH: usize = MAX_TOPIC_DEPTH;
pub const MAX_TOPIC_TITLE_LEN: usize = 200;
pub const MAX_TOPICS_PER_ROOM: usize = 5000;

/// Minimal topic shape for depth calculations without importing proto.
pub trait TopicDepthLike {
    fn topic_id(&self) -> &str;
    fn topic_parent_id(&self) -> Option<&str>;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TopicDepthError {
    #[error("topic tree exceeds max depth of {MAX_TOPIC_DEPTH}")]
    TooDeep,
}

/// Depth from root: root topics are depth 1.
pub fn topic_depth<T: TopicDepthLike>(topics: &[T], topic_id: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut current = Some(topic_id);
    while let Some(id) = current {
        depth += 1;
        let t = topics.iter().find(|t| t.topic_id() == id)?;
        current = t.topic_parent_id();
    }
    Some(depth)
}

/// Height of the subtree rooted at `topic_id`, counting the root as 1.
pub fn subtree_max_depth<T: TopicDepthLike>(topics: &[T], topic_id: &str) -> usize {
    let child_depths: Vec<usize> = topics
        .iter()
        .filter(|t| t.topic_parent_id() == Some(topic_id))
        .map(|c| subtree_max_depth(topics, c.topic_id()))
        .collect();
    1 + child_depths.into_iter().max().unwrap_or(0)
}

/// Reject placing a new child under `parent_id` (None = root).
pub fn validate_new_child_depth<T: TopicDepthLike>(
    topics: &[T],
    parent_id: Option<&str>,
) -> Result<(), TopicDepthError> {
    let new_depth = match parent_id {
        None => 1,
        Some(pid) => topic_depth(topics, pid)
            .map(|d| d + 1)
            .unwrap_or(MAX_TOPIC_DEPTH + 1),
    };
    if new_depth > MAX_TOPIC_DEPTH {
        return Err(TopicDepthError::TooDeep);
    }
    Ok(())
}

/// Reject moving `topic_id` under `new_parent_id` if the subtree would exceed the cap.
pub fn validate_move_depth<T: TopicDepthLike>(
    topics: &[T],
    topic_id: &str,
    new_parent_id: Option<&str>,
) -> Result<(), TopicDepthError> {
    let new_root_depth = match new_parent_id {
        None => 1,
        Some(pid) => topic_depth(topics, pid)
            .map(|d| d + 1)
            .unwrap_or(MAX_TOPIC_DEPTH + 1),
    };
    let subtree = subtree_max_depth(topics, topic_id);
    if new_root_depth + subtree - 1 > MAX_TOPIC_DEPTH {
        return Err(TopicDepthError::TooDeep);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportValidationError {
    #[error("imported tree is empty")]
    Empty,
    #[error("imported tree has {0} nodes; max is {MAX_IMPORT_TOPICS}")]
    TooManyTopics(usize),
    #[error("imported tree is {0} levels deep; max is {MAX_IMPORT_DEPTH}")]
    TooDeep(usize),
    #[error("a topic title is empty")]
    EmptyTitle,
    #[error("a topic title exceeds {MAX_TOPIC_TITLE_LEN} chars")]
    TitleTooLong,
}

/// Validate an imported topic tree before it touches the in-memory
/// model or the writer. Walks the full tree counting nodes + depth
/// and checking each title length.
pub fn validate_imported_topics<T: ImportedTopicLike>(
    topics: &[T],
) -> Result<(), ImportValidationError> {
    validate_imported_topics_with_base(topics, 0)
}

pub fn validate_imported_topics_with_base<T: ImportedTopicLike>(
    topics: &[T],
    base_depth: usize,
) -> Result<(), ImportValidationError> {
    if topics.is_empty() {
        return Err(ImportValidationError::Empty);
    }
    let mut total = 0usize;
    fn walk<T: ImportedTopicLike>(
        nodes: &[T],
        depth: usize,
        base_depth: usize,
        total: &mut usize,
    ) -> Result<(), ImportValidationError> {
        let absolute = base_depth + depth;
        if absolute > MAX_TOPIC_DEPTH {
            return Err(ImportValidationError::TooDeep(absolute));
        }
        for n in nodes {
            *total += 1;
            if *total > MAX_IMPORT_TOPICS {
                return Err(ImportValidationError::TooManyTopics(*total));
            }
            let title = n.title().trim();
            if title.is_empty() {
                return Err(ImportValidationError::EmptyTitle);
            }
            if title.chars().count() > MAX_TOPIC_TITLE_LEN {
                return Err(ImportValidationError::TitleTooLong);
            }
            walk(n.children(), depth + 1, base_depth, total)?;
        }
        Ok(())
    }
    walk(topics, 1, base_depth, &mut total)
}

/// Small trait so `validate_imported_topics` works against both the
/// proto type and unit-test fixtures without dragging proto into
/// this module.
pub trait ImportedTopicLike {
    fn title(&self) -> &str;
    fn children(&self) -> &[Self]
    where
        Self: Sized;
}

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

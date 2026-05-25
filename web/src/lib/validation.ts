// G.4 — single source of truth for the raise-hand topic word count
// on the client. The server's canonical reference is Rust's
// `str.split_whitespace()`, which matches Unicode whitespace
// (including U+00A0 NBSP, the U+2000..U+200A space block, and the
// U+3000 ideographic space) and does NOT split on U+200D zero-width
// joiner. The JavaScript `\s` character class is equivalent for these
// cases, so this implementation stays terse.
//
// Keep the implementation here aligned with
// `server/src/validation.rs::count_topic_words` — both are exercised
// by parallel test suites on each side.

/// Count whitespace-delimited words in `input`. Empty / whitespace-only
/// strings return 0. Multiple whitespace runs (including NBSP) count as
/// a single separator. Zero-width joiners do NOT split words.
export function countTopicWords(input: string): number {
  const trimmed = input.trim();
  if (trimmed.length === 0) return 0;
  return trimmed.split(/\s+/).filter((s) => s.length > 0).length;
}

/// Maximum allowed words in a raise-hand topic. Mirrors
/// `MAX_RAISE_HAND_TOPIC_WORDS` on the server.
export const MAX_RAISE_HAND_TOPIC_WORDS = 10;

/// Maximum allowed length (chars) of a raise-hand topic.
export const MAX_RAISE_HAND_TOPIC_LEN = 80;

/// Predicate the composer uses to enable/disable the submit button.
export function isValidRaiseHandTopic(topic: string): boolean {
  const trimmed = topic.trim();
  if (trimmed.length === 0) return false;
  if (topic.length > MAX_RAISE_HAND_TOPIC_LEN) return false;
  return countTopicWords(topic) <= MAX_RAISE_HAND_TOPIC_WORDS;
}

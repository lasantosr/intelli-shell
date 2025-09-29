/// Represents a component of a fuzzy search query.
///
/// A query is parsed into a sequence of `FuzzyMatch` items.
///
/// These items are implicitly ANDed together when performing a search, unless they are part of an `Or` variant.
#[derive(PartialEq, Eq, Debug)]
pub enum FuzzyMatch<'a> {
    /// A single search term with a specific matching strategy
    Term(FuzzyTerm<'a>),
    /// A collection of terms where at least one must match (logical OR)
    Or(Vec<FuzzyTerm<'a>>),
}

/// Represents an individual search term and its associated matching semantics
#[derive(PartialEq, Eq, Debug)]
pub struct FuzzyTerm<'a> {
    /// The kind of fuzzy matching to apply to this term
    pub kind: FuzzyTermKind,
    /// The string slice representing the term to match
    pub term: &'a str,
}

/// Defines the different kinds of matching strategies for a [`FuzzyTerm`].
///
/// The specific syntax used in the query string determines the `FuzzyTermKind`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FuzzyTermKind {
    /// Chars must be found in the same order, but not necessarily consecutively
    #[default]
    Fuzzy,
    /// The search input must contain the term exactly
    Exact,
    /// The term must appear as a whole word, respecting word boundaries
    ExactBoundary,
    /// The search input must start with this term
    PrefixExact,
    /// The search input must end with this term
    SuffixExact,
    /// The search input must *not* contain this term exactly
    InverseExact,
    /// The search input must *not* start with this term
    InversePrefixExact,
    /// The search input must *not* end with this term
    InverseSuffixExact,
}

/// Parses a fuzzy search query string into a vector of [`FuzzyMatch`] items.
///
/// The query is split by spaces. Terms are implicitly ANDed.
///
/// A `|` token acts as an OR operator, grouping the term before it with the terms after it until the next non-OR term
/// or end of query.
///
/// # Examples
///
/// ```rust
/// # use intelli_shell::utils::{FuzzyMatch, FuzzyTerm, FuzzyTermKind, parse_fuzzy_query};
/// assert_eq!(
///     parse_fuzzy_query("foo bar"),
///     vec![
///         FuzzyMatch::Term(FuzzyTerm { kind: FuzzyTermKind::Fuzzy, term: "foo" }),
///         FuzzyMatch::Term(FuzzyTerm { kind: FuzzyTermKind::Fuzzy, term: "bar" }),
///     ]
/// );
///
/// assert_eq!(
///     parse_fuzzy_query("foo | bar"),
///     vec![FuzzyMatch::Or(vec![
///         FuzzyTerm { kind: FuzzyTermKind::Fuzzy, term: "foo" },
///         FuzzyTerm { kind: FuzzyTermKind::Fuzzy, term: "bar" },
///     ])],
/// );
///
/// assert_eq!(
///     parse_fuzzy_query("^core go$ | rb$ | py$"),
///     vec![
///         FuzzyMatch::Term(FuzzyTerm { kind: FuzzyTermKind::PrefixExact, term: "core" }),
///         FuzzyMatch::Or(vec![
///             FuzzyTerm { kind: FuzzyTermKind::SuffixExact, term: "go" },
///             FuzzyTerm { kind: FuzzyTermKind::SuffixExact, term: "rb" },
///             FuzzyTerm { kind: FuzzyTermKind::SuffixExact, term: "py" },
///         ]),
///     ]
/// );
pub fn parse_fuzzy_query(query: &str) -> Vec<FuzzyMatch<'_>> {
    let mut matches: Vec<FuzzyMatch<'_>> = Vec::new();
    let mut current_or_group: Vec<FuzzyTerm<'_>> = Vec::new();
    // Indicates if the last processed token was a regular term (true) or a `|` operator / start of query (false)
    let mut last_token_was_term = false;

    for token_str in query.split_whitespace() {
        if token_str == "|" {
            // This token is an OR operator
            if last_token_was_term {
                // The `|` operator applies to the previous valid term
                if current_or_group.is_empty() {
                    // The previous item was a single term; start a new OR group with it
                    if let Some(FuzzyMatch::Term(term)) = matches.pop() {
                        current_or_group.push(term);
                    } else {
                        last_token_was_term = false;
                        continue;
                    }
                }
                // If current_or_group was not empty, this `|` continues an existing OR sequence
                last_token_was_term = false;
            } else {
                // Consecutive `|` tokens or `|` after an ignored (empty) term or at query start
                last_token_was_term = false;
            }
        } else {
            // This token is a potential search term
            if let Some(parsed_term) = parse_individual_term(token_str) {
                // A valid, non-empty term was parsed
                if !last_token_was_term && !current_or_group.is_empty() {
                    // Previous significant token was `|`, and we are in an active OR group. Add this term to it.
                    current_or_group.push(parsed_term);
                } else {
                    // This term starts a new single item, or follows another single item
                    // Finalize any pending OR group before adding this new term as a single
                    if !current_or_group.is_empty() {
                        matches.push(FuzzyMatch::Or(std::mem::take(&mut current_or_group)));
                    }
                    matches.push(FuzzyMatch::Term(parsed_term));
                }
                last_token_was_term = true;
            } else {
                last_token_was_term = false;
            }
        }
    }

    // After iterating through all tokens, if an OR group is still pending, finalize it.
    if !current_or_group.is_empty() {
        matches.push(FuzzyMatch::Or(current_or_group));
    }

    matches
}

/// Parses an individual token string from the query into a [`FuzzyTerm`].
///
/// It identifies special characters (`'`, `^`, `$`, `!`) to determine the [`FuzzyTermKind`] and extracts the actual
/// term string.
///
/// Returns `None` if the resulting term string is empty after stripping special characters.
fn parse_individual_term(token_str: &str) -> Option<FuzzyTerm<'_>> {
    // Check for most specific patterns first
    let fuzzy = if let Some(term) = token_str.strip_prefix("!^") {
        // Handles "!^term"
        FuzzyTerm {
            kind: FuzzyTermKind::InversePrefixExact,
            term,
        }
    } else if token_str.starts_with('!') && token_str.ends_with('$') {
        // Handles "!term$"
        FuzzyTerm {
            kind: FuzzyTermKind::InverseSuffixExact,
            term: &token_str[1..(token_str.len() - 1)],
        }
    } else if token_str.starts_with('\'') && token_str.ends_with('\'') {
        // Handles "'term'"
        FuzzyTerm {
            kind: FuzzyTermKind::ExactBoundary,
            term: &token_str[1..(token_str.len() - 1)],
        }
    } else if let Some(term) = token_str.strip_prefix('\'') {
        // Handles "'term"
        FuzzyTerm {
            kind: FuzzyTermKind::Exact,
            term,
        }
    } else if let Some(term) = token_str.strip_prefix('^') {
        // Handles "^term"
        FuzzyTerm {
            kind: FuzzyTermKind::PrefixExact,
            term,
        }
    } else if let Some(term) = token_str.strip_suffix('$') {
        // Handles "term$"
        FuzzyTerm {
            kind: FuzzyTermKind::SuffixExact,
            term,
        }
    } else if let Some(term) = token_str.strip_prefix('!') {
        // Handles "!term"
        FuzzyTerm {
            kind: FuzzyTermKind::InverseExact,
            term,
        }
    } else {
        // Default: Fuzzy match for "term"
        FuzzyTerm {
            kind: FuzzyTermKind::Fuzzy,
            term: token_str,
        }
    };
    // Skip empty terms
    if fuzzy.term.is_empty() { None } else { Some(fuzzy) }
}

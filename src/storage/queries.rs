use itertools::Itertools;
use regex::Regex;
use sea_query::{
    Alias, Asterisk, BinOper, CommonTableExpression, Cond, Expr, ExprTrait, Func, FunctionCall, Iden, IntoIden,
    JoinType, LikeExpr, Order, Query, SelectStatement, SimpleExpr, TableRef, UnionType, WindowStatement, WithClause,
    extension::sqlite::SqliteExpr,
};
use strum::VariantArray;

use crate::{
    config::{SearchCommandTuning, SearchCommandsTextTuning},
    errors::{Result, UserFacingError},
    model::{SearchCommandsFilter, SearchMode},
    utils::{FuzzyMatch, FuzzyTerm, FuzzyTermKind, flatten_str, parse_fuzzy_query},
};

/// Default limit for the number of rows returned when querying
pub const QUERY_LIMIT: u64 = 500;

/// A special, high rank assigned to command-like matches to ensure they are always prioritized.
/// This value is used to identify these results so they can be separated before normalization.
pub const TEMPLATE_MATCH_RANK: f64 = 1000000.0;

/// Name for the column that determines if a cmd is present on the workspace commands file
pub const IS_WORKSPACE: &str = "is_workspace_command";

/// Constructs the main SQL query to find command tags based on multiple criteria.
///
/// The query returns three columns: tag, usage and exact_match
pub fn query_find_tags(
    filter: SearchCommandsFilter,
    tag_prefix: Option<String>,
    tuning: &SearchCommandTuning,
    workspace_tables_loaded: bool,
) -> Result<SelectStatement> {
    let cleaned_filter = filter.cleaned();
    let filter_tags = cleaned_filter.tags.clone();

    // Build the query to fetch commands matching the given filter
    let (mut with, command_query, _) = query_commands_filtered(cleaned_filter, None, tuning, workspace_tables_loaded)?;
    let base_commands_cte_name = "commands_filtered";
    let base_commands_cte = CommonTableExpression::new()
        .table_name(base_commands_cte_name)
        .query(command_query)
        .to_owned();
    with.cte(base_commands_cte);

    let t = "t";
    let c = "c";
    let usage = "usage";
    let exact_match = "exact_match";
    Ok(Query::select()
        .expr(Expr::col((t, "value")))
        .expr_as(Expr::col((c, Command::Rowid)).count(), usage)
        // Add an exact_match column
        .apply(|s| {
            if let Some(prefix) = tag_prefix {
                // If there's a tag prefix, filter by it 
                s.and_where(
                    Expr::col((t, "value")).like(LikeExpr::new(format!("{}%", escape_like(&prefix))).escape('\\')),
                )
                .expr_as(
                    Expr::case(Expr::col((t, "value")).eq(prefix), 1).finally(0),
                    exact_match,
                );
            } else {
                s.expr_as(Expr::cust("0"), exact_match);
            }
        })
        .apply_if(filter_tags, |s, tags| {
            // Exclude filtered tags from output
            s.and_where(Expr::col((t, "value")).is_not_in(tags));
        })
        .from_as(base_commands_cte_name, c)
        .from(TableRef::FunctionCall(
            Func::cust("json_each").arg(Expr::col((c, Command::Tags))),
            t.into_iden(),
        ))
        .group_by_col((t, "value"))
        .order_by(exact_match, Order::Desc)
        .order_by(usage, Order::Desc)
        .order_by((t, "value"), Order::Asc)
        .limit(QUERY_LIMIT)
        .with_cte(with)
        .take())
}

/// Constructs the main SQL query to find and score commands based on multiple criteria.
///
/// This function orchestrates the query building by creating a multi-stage pipeline using CTEs.
/// The goal is to produce a result set where each command is annotated with three distinct scores for final re-ranking.
///
/// The query pipeline is as follows:
/// 1. CTE **`commands_filtered`**: First, it calls `query_commands_filtered` to get an initial set of commands matching
///    the text search (`auto`, `exact`, etc.) and static filters (tags, category). This CTE is responsible for
///    calculating the `text_score`.
///
/// 2. CTE **`commands_usage`**: This builds upon the first CTE, calculating the two contextual scores based on the
///    "Dual Score" model:
///     - `path_score`: The quality of the path match (exact, ancestor, etc.)
///     - `usage_score`: The log-scaled total usage count of the command
///
/// 3. **Final `SELECT`**: The final query joins these CTEs to produce a list of commands, each with its `text_score`,
///    `path_score`, and `usage_score` ready for the external re-ranking logic.
///
/// Additional CTEs might be present to compute the `commands_filtered` one.
pub fn query_find_commands(
    filter: SearchCommandsFilter,
    working_path: impl Into<String>,
    tuning: &SearchCommandTuning,
    workspace_tables_loaded: bool,
) -> Result<SelectStatement> {
    let cleaned_filter = filter.cleaned();
    let working_path = working_path.into();

    // Build the query to fetch commands matching the given filter and convert to a cte
    let c = "c";
    let usage_score = "usage_score";
    let path_score = "path_score";
    let text_score = "text_score";
    let (mut with, command_query, has_text_rank) =
        query_commands_filtered(cleaned_filter, Some(text_score), tuning, workspace_tables_loaded)?;
    let base_commands_cte_name = "commands_filtered";
    let base_commands_cte = CommonTableExpression::new()
        .table_name(base_commands_cte_name)
        .query(command_query)
        .to_owned();
    with.cte(base_commands_cte);

    // Build another cte with the commands usage
    let u = "u";
    let path_relevance = Expr::case(Expr::col((u, CommandUsage::Path)).eq(&working_path), tuning.path.exact)
        .case(
            Expr::cust_with_exprs(
                "? LIKE ? || '/%'",
                vec![Expr::val(&working_path), Expr::col((u, CommandUsage::Path))],
            ),
            tuning.path.ancestor,
        )
        .case(
            Expr::cust_with_exprs(
                "? LIKE ? || '/%'",
                vec![Expr::col((u, CommandUsage::Path)), Expr::val(&working_path)],
            ),
            tuning.path.descendant,
        )
        .finally(tuning.path.unrelated);
    let usage_query = Query::select()
        .expr(Expr::col((c, Command::Rowid)))
        .expr_as(
            CustomFunc::log(
                Func::sum(Func::coalesce([
                    path_relevance.clone().mul(Expr::col((u, CommandUsage::UsageCount))),
                    Expr::cust("0"),
                ]))
                .add(Expr::cust("1")),
            ),
            usage_score,
        )
        .expr_as(Func::max(path_relevance), path_score)
        .from_as(base_commands_cte_name, c)
        .join_as(
            JoinType::LeftJoin,
            CommandUsage::Table,
            u,
            Expr::col((c, Command::Id)).eq(Expr::col((u, CommandUsage::CommandId))),
        )
        .group_by_col((c, Command::Rowid))
        .take();
    let usage_cte_name = "commands_usage";
    let usage_cte = CommonTableExpression::new()
        .table_name(usage_cte_name)
        .query(usage_query)
        .to_owned();
    with.cte(usage_cte);

    // Build the final query
    Ok(Query::select()
        .apply(|s| {
            for field in Command::VARIANTS {
                let field = *field;
                if field != Command::Table {
                    s.expr(Expr::col((c, field)));
                }
            }
        })
        .expr(Expr::col((c, IS_WORKSPACE)))
        .expr(Expr::col((u, usage_score)))
        .expr(Expr::col((u, path_score)))
        .apply(|s| {
            if has_text_rank {
                s.expr(Expr::col((c, text_score)));
            } else {
                s.expr_as(Expr::cust("0.0"), text_score);
            }
        })
        .from_as(base_commands_cte_name, c)
        .join_as(
            JoinType::Join,
            usage_cte_name,
            u,
            Expr::col((c, Command::Rowid)).eq(Expr::col((u, Command::Rowid))),
        )
        .order_by(IS_WORKSPACE, Order::Desc)
        .order_by(text_score, Order::Desc)
        .order_by(path_score, Order::Desc)
        .order_by(usage_score, Order::Desc)
        .order_by(Command::Id, Order::Asc)
        .limit(QUERY_LIMIT)
        .with_cte(with)
        .take())
}

/// Builds a query to select commands based on the provided filter.
///
/// The returned query selects all fields from the `command` table, plus an optional rank field if provided.
/// It doesn't include `ORDER BY` or `LIMIT` clauses.
///
/// ## Returns
/// - The required WITH clause with the CTEs
/// - The main select with commands
/// - Whether the main select has a rank column
fn query_commands_filtered(
    cleaned_filter: SearchCommandsFilter,
    rank_field_name: Option<&'static str>,
    tuning: &SearchCommandTuning,
    workspace_tables_loaded: bool,
) -> Result<(WithClause, SelectStatement, bool)> {
    // Get the base query and determine if it has a rank column from the inner calls
    // - the `base_query` will be a UNION of global and workspace results if workspace tables are loaded
    // - the `with` clause will contain all CTEs generated by the underlying text searches
    let (ctes, base_query, has_rank) = if cleaned_filter.search_term.is_some() {
        // If there's a search term, we need to perform a text search
        let (mut global_ctes, mut global_query, has_rank) = query_commands_with_text_search(
            cleaned_filter.clone(),
            rank_field_name,
            tuning,
            Command::Table,
            CommandFts::Table,
            CommandFuzzyFts::Table,
            false,
        )?;

        // If no workspace tables are loaded, we can return the global results directly
        if !workspace_tables_loaded {
            let mut with = WithClause::new();
            for cte in global_ctes {
                with.cte(cte);
            }
            return Ok((with, global_query, has_rank));
        }

        // Otherwise, we need to query workspace commands as well
        let (mut workspace_ctes, workspace_query, _) = query_commands_with_text_search(
            cleaned_filter,
            rank_field_name,
            tuning,
            WorkspaceCommand::Table,
            WorkspaceCommandFts::Table,
            WorkspaceCommandFuzzyFts::Table,
            true,
        )?;

        // Combine CTEs and queries
        global_ctes.append(&mut workspace_ctes);
        let union_query = global_query.union(UnionType::All, workspace_query).take();
        (global_ctes, union_query, has_rank)
    } else {
        // If there's no search term, we just need to filter by category, source, and tags
        let mut global_query = query_commands_filtered_by(
            cleaned_filter.category.clone(),
            cleaned_filter.source.clone(),
            cleaned_filter.tags.clone(),
            Command::Table,
            false,
        );

        // If no workspace tables are loaded, we can return the global results directly
        if !workspace_tables_loaded {
            return Ok((WithClause::new(), global_query, false));
        }

        // Otherwise, we need to query workspace commands as well
        let workspace_query = query_commands_filtered_by(
            cleaned_filter.category,
            cleaned_filter.source,
            cleaned_filter.tags,
            WorkspaceCommand::Table,
            true,
        );
        let union_query = global_query.union(UnionType::All, workspace_query).take();
        (Vec::new(), union_query, false)
    };

    // If workspace tables are loaded, apply deduplication
    let mut with = WithClause::new();
    for cte in ctes {
        with.cte(cte);
    }
    let union_cte_name = "commands_unified";
    let union_cte = CommonTableExpression::new()
        .table_name(union_cte_name)
        .query(base_query)
        .to_owned();
    with.cte(union_cte);

    // Create a second CTE that ranks the results for deduplication
    let dedup_rank_col = "_rank";
    let count_col = "_count";
    let ranked_cte_name = "commands_ranked";
    let ranked_query = Query::select()
        .expr(Expr::col(Asterisk))
        // Rank rows: global (non-workspace) commands get rank 1, workspace commands get rank 2
        .expr_window_as(
            CustomFunc::row_number(),
            WindowStatement::partition_by_custom(r#"TRIM("cmd")"#)
                .order_by(IS_WORKSPACE, Order::Asc).take(),
            dedup_rank_col,
        )
        // Count cmd occurences: 2 if global and workspace
        .expr_window_as(
            Expr::col(Asterisk).count(),
            WindowStatement::partition_by_custom(r#"TRIM("cmd")"#),
            count_col,
        )
        .from(union_cte_name)
        .take();
    let ranked_cte = CommonTableExpression::new()
        .table_name(ranked_cte_name)
        .query(ranked_query)
        .to_owned();
    with.cte(ranked_cte);

    // Build the final query from the ranked CTE
    let final_query = Query::select()
        .apply(|s| {
            for field in Command::VARIANTS {
                let field = *field;
                if field != Command::Table {
                    s.expr(Expr::col(field));
                }
            }
        })
        // If the command is from workspace file or it has more than one occurrence, mark it as a workspace command
        .expr_as(Expr::col(IS_WORKSPACE).or(Expr::col(count_col).gt(1)), IS_WORKSPACE)
        .apply(|s| {
            if has_rank && let Some(rank_field) = rank_field_name {
                s.expr(Expr::col(rank_field));
            }
        })
        .from(ranked_cte_name)
        // Select only the top-ranked row for each command, enforcing global-over-workspace priority
        .and_where(Expr::col(dedup_rank_col).eq(1))
        .take();

    Ok((with, final_query, has_rank))
}

/// A helper to perform text search on a given set of command tables.
/// This abstracts the logic for searching global or workspace commands.
fn query_commands_with_text_search(
    cleaned_filter: SearchCommandsFilter,
    rank_field_name: Option<&'static str>,
    tuning: &SearchCommandTuning,
    command_table: impl Iden + Copy + 'static,
    fts_table: impl Iden + Copy + 'static,
    fuzzy_fts_table: impl Iden + Copy + 'static,
    workspace_tables: bool,
) -> Result<(Vec<CommonTableExpression>, SelectStatement, bool)> {
    let SearchCommandsFilter {
        category,
        source,
        tags,
        search_mode,
        search_term,
    } = cleaned_filter;

    // This function must be called with a search term
    let term = search_term.expect("search_term should not be None here");

    // Query commands with given filters
    let base_query = query_commands_filtered_by(category, source, tags, command_table, workspace_tables);

    // Build a CTE from the base query in order to extend it
    let mut ctes = Vec::new();
    let base_cte_name = if workspace_tables {
        "workspace_commands_base"
    } else {
        "commands_base"
    };
    let base_cte = CommonTableExpression::new()
        .table_name(base_cte_name)
        .query(base_query)
        .to_owned();
    ctes.push(base_cte);

    // Build the final query, depending on the search mode
    let (final_query, has_rank) = match search_mode {
        SearchMode::Relaxed => {
            let (relaxed_query, has_rank) =
                query_commands_relaxed(base_cte_name, rank_field_name, &term, &tuning.text, fuzzy_fts_table);
            (relaxed_query, has_rank)
        }
        SearchMode::Exact => {
            let (exact_query, has_rank) =
                query_commands_exact(base_cte_name, rank_field_name, &term, &tuning.text, fts_table);
            (exact_query, has_rank)
        }
        SearchMode::Regex => {
            let regex = Regex::new(&term).map_err(|err| {
                tracing::warn!("Invalid regex: {err}");
                UserFacingError::InvalidRegex
            })?;
            let regex_query = query_commands_regex(base_cte_name, regex);
            (regex_query, false)
        }
        SearchMode::Fuzzy => {
            let fuzzy_matches = parse_fuzzy_query(&term);
            if fuzzy_matches.is_empty() {
                return Err(UserFacingError::InvalidFuzzy.into());
            }
            let fuzzy_query = query_commands_fuzzy(base_cte_name, fuzzy_matches);
            (fuzzy_query, false)
        }
        SearchMode::Auto => {
            let (mut auto_ctes, auto_query, has_rank) = query_commands_auto(
                base_cte_name,
                rank_field_name,
                term,
                &tuning.text,
                fts_table,
                fuzzy_fts_table,
                workspace_tables,
            );
            ctes.append(&mut auto_ctes);
            (auto_query, has_rank)
        }
    };

    Ok((ctes, final_query, has_rank))
}

/// Query commands, filtered by the given criteria
fn query_commands_filtered_by(
    category: Option<Vec<String>>,
    source: Option<String>,
    tags: Option<Vec<String>>,
    table: impl Iden + 'static,
    workspace_table: bool,
) -> SelectStatement {
    let c = "c";
    Query::select()
        .expr(Expr::col((c, Command::Rowid)))
        .expr(Expr::col((c, Asterisk)))
        .expr_as(Expr::val(workspace_table), IS_WORKSPACE)
        .from_as(table, c)
        // Filter by category
        .apply_if(category, |s, category| {
            s.and_where(Expr::col((c, Command::Category)).is_in(category));
        })
        // Filter by source
        .apply_if(source, |s, source| {
            s.and_where(Expr::col((c, Command::Source)).eq(source));
        })
        // Filter by tags
        .apply_if(tags, |s, tags| {
            // Count the total number of tags searched
            let tags_len = tags.len() as i32;

            // This subquery counts how many of the provided tags are present in the command's JSON tags array
            let jt = "jt";
            let subquery = Query::select()
                .expr(Expr::col((jt, "value")).count_distinct())
                .from_function(Func::cust("json_each").arg(Expr::col((c, Command::Tags))), jt)
                .and_where(Expr::col((jt, "value")).is_in(tags))
                .take();

            // Ensure the command's tags are not NULL, because we're looking for commands with tags
            s.and_where(Expr::col(("c", Command::Tags)).is_not_null())
                // The count from the subquery must equal the number of tags passed in, ensuring the command matches all
                .and_where(Expr::SubQuery(None, Box::new(subquery.into())).eq(tags_len));
        })
        .take()
}

/// Query commands from the given commands table, filtering those relaxed matching the given term
fn query_commands_relaxed(
    base_table: impl Iden + 'static,
    rank_field_name: Option<&'static str>,
    term: impl AsRef<str>,
    tuning: &SearchCommandsTextTuning,
    fuzzy_fts_table: impl Iden + Copy + 'static,
) -> (SelectStatement, bool) {
    let mut has_rank = false;
    // Flatten the input term to remove any non-alphanumeric characters
    let term_flat = flatten_str(term.as_ref());
    // Split the term into words and build the FTS query
    let search_words = term_flat.split_whitespace().collect::<Vec<_>>();
    let fts_query = search_words.into_iter().map(escape_fts).join(" OR ");

    // Prepare the query
    let c = "c";
    let fts = "fts";
    (
        Query::select()
            .expr(Expr::col((c, Asterisk)))
            .from_as(base_table, c)
            .apply(|s| {
                if !fts_query.is_empty() {
                    s.apply_if(rank_field_name, |s, rank_field| {
                        has_rank = true;
                        s.expr_as(
                            CustomFunc::neg_bm25(Expr::col((fts, fuzzy_fts_table)), tuning.command, tuning.description),
                            Alias::new(rank_field),
                        );
                    })
                    .join_as(
                        JoinType::InnerJoin,
                        fuzzy_fts_table,
                        fts,
                        Expr::col((c, Command::Rowid)).eq(Expr::col((fts, CommandFuzzyFts::Rowid))),
                    )
                    .and_where(Expr::col((fts, fuzzy_fts_table)).matches(fts_query));
                }
            })
            .take(),
        has_rank,
    )
}

/// Query commands from the given commands table, filtering those exactly matching the given term
fn query_commands_exact(
    base_table: impl Iden + 'static,
    rank_field_name: Option<&'static str>,
    term: impl AsRef<str>,
    tuning: &SearchCommandsTextTuning,
    fts_table: impl Iden + Copy + 'static,
) -> (SelectStatement, bool) {
    let mut has_rank = false;
    // Escape the term for FTS
    let fts_query = escape_fts(term.as_ref());

    // Prepare the query
    let c = "c";
    let fts = "fts";
    (
        Query::select()
            .expr(Expr::col((c, Asterisk)))
            .apply_if(rank_field_name, |s, rank_field| {
                has_rank = true;
                s.expr_as(
                    CustomFunc::neg_bm25(Expr::col((fts, fts_table)), tuning.command, tuning.description),
                    Alias::new(rank_field),
                );
            })
            .from_as(base_table, c)
            .join_as(
                JoinType::InnerJoin,
                fts_table,
                fts,
                Expr::col((c, Command::Rowid)).eq(Expr::col((fts, CommandFts::Rowid))),
            )
            .and_where(Expr::col((fts, fts_table)).matches(fts_query))
            .take(),
        has_rank,
    )
}

/// Query commands from the given commands table, filtering those matching the given regex
fn query_commands_regex(base_table: impl Iden + 'static, regex: Regex) -> SelectStatement {
    let c = "c";
    Query::select()
        .expr(Expr::col((c, Asterisk)))
        .from_as(base_table, c)
        .and_where(Expr::col((c, Command::Cmd)).regexp(regex.as_str()))
        .take()
}

/// Query commands from the given commands table, filtering those fuzzy matching the given matches
fn query_commands_fuzzy(base_table: impl Iden + 'static, fuzzy_matches: Vec<FuzzyMatch<'_>>) -> SelectStatement {
    let c = "c";
    // Translate fuzzy terms into where conditions
    let mut all_conditions = Cond::all();
    for fuzzy_match in fuzzy_matches {
        match fuzzy_match {
            // For a single term, add its condition directly to the AND group
            FuzzyMatch::Term(ft) => {
                all_conditions = all_conditions.add(expr_for_fuzzy_term(&ft, c));
            }
            // For an OR group, create a nested condition group
            FuzzyMatch::Or(or_terms) => {
                let mut or_conditions = Cond::any();
                for ft in or_terms {
                    or_conditions = or_conditions.add(expr_for_fuzzy_term(&ft, c));
                }
                if !or_conditions.is_empty() {
                    all_conditions = all_conditions.add(or_conditions);
                }
            }
        }
    }
    // Build the query
    Query::select()
        .expr(Expr::col((c, Asterisk)))
        .from_as(base_table, c)
        .cond_where(all_conditions)
        .take()
}

/// Query commands from the given commands table, filtering those matching the given term with a custom algorithm
fn query_commands_auto(
    base_table: &'static str,
    rank_field_name: Option<&'static str>,
    term: impl AsRef<str>,
    tuning: &SearchCommandsTextTuning,
    fts_table: impl Iden + Copy + 'static,
    fuzzy_fts_table: impl Iden + Copy + 'static,
    workspace_tables: bool,
) -> (Vec<CommonTableExpression>, SelectStatement, bool) {
    // Separate words from negated terms
    let mut words: Vec<&str> = Vec::new();
    let mut negated_terms: Vec<&str> = Vec::new();
    for word in term.as_ref().split_whitespace() {
        if word.starts_with('!') && word.len() > 1 {
            negated_terms.push(&word[1..]);
        } else if !word.is_empty() && word != "!" {
            words.push(word);
        }
    }

    // Convert negated terms to condition
    let negated_terms_cond = if negated_terms.is_empty() {
        None
    } else {
        let mut cond = Cond::all();
        for neg_term in negated_terms {
            let pattern = format!("%{}%", escape_like(neg_term));
            cond = cond.add(Expr::col(Command::Cmd).not_like(LikeExpr::new(pattern.clone()).escape('\\')));
            cond = cond.add(Expr::from(Func::coalesce([
                Expr::col(Command::Description).not_like(LikeExpr::new(pattern).escape('\\')),
                Expr::cust("TRUE"),
            ])));
        }
        Some(cond)
    };

    // If there are no words to search for
    if words.is_empty() {
        // Apply only the negation filters, if any
        let query = Query::select()
            .expr(Expr::col(Asterisk))
            .from(base_table)
            .apply_if(negated_terms_cond, |s, cond| {
                s.cond_where(cond);
            })
            .take();
        (Vec::new(), query, false)
    } else {
        let c = "c";
        // Split both term and flattened term into words
        let term_flat = flatten_str(words.join(" "));
        let flat_words = term_flat.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        let mut ctes = Vec::new();
        let mut union_selects = Vec::new();
        let mut has_rank = false;

        // Determine the base table name to use
        let base_from_table = if let Some(cond) = negated_terms_cond {
            // If there are negated terms, create a preliminary CTE to filter them out
            let cte_name = if workspace_tables {
                "workspace_commands_excluding_negated"
            } else {
                "commands_excluding_negated"
            };
            let negated_terms_query = Query::select()
                .expr(Expr::col(Asterisk))
                .from(base_table)
                .cond_where(cond)
                .take();

            let base_cte = CommonTableExpression::new()
                .table_name(cte_name)
                .query(negated_terms_query)
                .to_owned();
            ctes.push(base_cte);
            cte_name
        } else {
            // Otherwise just use the original base table
            base_table
        };

        // 1. Match all prefixes
        let fts1_term = words
            .iter()
            .map(|w| format!("{}*", escape_fts(w)))
            .collect::<Vec<_>>()
            .join(" ");
        if !fts1_term.is_empty() {
            let fts1 = if workspace_tables { "workspace_fts1" } else { "fts1" };
            let fts1_query = Query::select()
                .expr(Expr::col((c, Asterisk)))
                .apply_if(rank_field_name, |s, rank_field| {
                    has_rank = true;
                    s.expr_as(
                        Expr::value(tuning.auto.prefix).mul(CustomFunc::neg_bm25(
                            Expr::col((fts1, fts_table)),
                            tuning.command,
                            tuning.description,
                        )),
                        rank_field,
                    );
                })
                .from_as(base_from_table, "c")
                .join_as(
                    JoinType::InnerJoin,
                    fts_table,
                    fts1,
                    Expr::col((c, Command::Rowid)).equals((fts1, CommandFts::Rowid)),
                )
                .and_where(Expr::col((fts1, fts_table)).matches(fts1_term))
                .limit(QUERY_LIMIT)
                .take();

            let fts1_cte = CommonTableExpression::new()
                .table_name(fts1)
                // fts cte must be materialized to avoid query planner optimizations, bm25 can't be used in those contexts
                .materialized(true)
                .query(fts1_query)
                .to_owned();
            ctes.push(fts1_cte);
            union_selects.push(Query::select().expr(Expr::col(Asterisk)).from(fts1).take());
        }

        // 2. Fuzzy match all words
        let fts2_term = flat_words.iter().map(|w| escape_fts(w)).join(" ");
        if !fts2_term.is_empty() {
            let fts2 = if workspace_tables { "workspace_fts2" } else { "fts2" };
            let fts2_query = Query::select()
                .expr(Expr::col((c, Asterisk)))
                .apply_if(rank_field_name, |s, rank_field| {
                    has_rank = true;
                    s.expr_as(
                        Expr::value(tuning.auto.fuzzy).mul(CustomFunc::neg_bm25(
                            Expr::col((fts2, fuzzy_fts_table)),
                            tuning.command,
                            tuning.description,
                        )),
                        rank_field,
                    );
                })
                .from_as(base_from_table, "c")
                .join_as(
                    JoinType::InnerJoin,
                    fuzzy_fts_table,
                    fts2,
                    Expr::col((c, Command::Rowid)).equals((fts2, CommandFuzzyFts::Rowid)),
                )
                .and_where(Expr::col((fts2, fuzzy_fts_table)).matches(fts2_term))
                .limit(QUERY_LIMIT)
                .take();

            let fts2_cte = CommonTableExpression::new()
                .table_name(fts2)
                // fts cte must be materialized to avoid query planner optimizations, bm25 can't be used in those contexts
                .materialized(true)
                .query(fts2_query)
                .to_owned();
            ctes.push(fts2_cte);
            union_selects.push(Query::select().expr(Expr::col(Asterisk)).from(fts2).take());
        }

        // 3. Relaxed fuzzy match
        let fts3_term = flat_words.iter().map(|w| escape_fts(w)).join(" OR ");
        if !fts3_term.is_empty() {
            let fts3 = if workspace_tables { "workspace_fts3" } else { "fts3" };
            let fts3_query = Query::select()
                .expr(Expr::col((c, Asterisk)))
                .apply_if(rank_field_name, |s, rank_field| {
                    has_rank = true;
                    s.expr_as(
                        Expr::value(tuning.auto.relaxed).mul(CustomFunc::neg_bm25(
                            Expr::col((fts3, fuzzy_fts_table)),
                            tuning.command,
                            tuning.description,
                        )),
                        rank_field,
                    );
                })
                .from_as(base_from_table, "c")
                .join_as(
                    JoinType::InnerJoin,
                    fuzzy_fts_table,
                    fts3,
                    Expr::col((c, Command::Rowid)).equals((fts3, CommandFuzzyFts::Rowid)),
                )
                .and_where(Expr::col((fts3, fuzzy_fts_table)).matches(fts3_term))
                .limit(QUERY_LIMIT)
                .take();

            let fts3_cte = CommonTableExpression::new()
                .table_name(fts3)
                // fts cte must be materialized to avoid query planner optimizations, bm25 can't be used in those contexts
                .materialized(true)
                .query(fts3_query)
                .to_owned();
            ctes.push(fts3_cte);
            union_selects.push(Query::select().expr(Expr::col(Asterisk)).from(fts3).take());
        }

        // 4. Template (Command-Like) Match
        if words.len() > 1 {
            let template_select = Query::select()
                .expr(Expr::col((c, Asterisk)))
                .apply_if(rank_field_name, |s, rank_field| {
                    has_rank = true;
                    s.expr_as(Expr::value(TEMPLATE_MATCH_RANK), rank_field);
                })
                .from_as(base_from_table, c)
                // Pre-filter commands starting with the first word, to avoid custom function call on every row
                .and_where(
                    Expr::col((c, Command::Cmd))
                        .like(LikeExpr::new(format!("{}%", escape_like(words.first().unwrap()))).escape('\\')),
                )
                // The original search term (excluding negated terms) matches a regex of the command
                .and_where(Expr::val(words.join(" ")).regexp(CustomFunc::cmd_to_regex(Expr::col((c, Command::Cmd)))))
                .limit(QUERY_LIMIT)
                .take();

            let template_name = if workspace_tables {
                "workspace_template_commands"
            } else {
                "template_commands"
            };
            let template_cte = CommonTableExpression::new()
                .table_name(template_name)
                .query(template_select)
                .to_owned();
            ctes.push(template_cte);
            union_selects.push(Query::select().expr(Expr::col(Asterisk)).from(template_name).take());
        }

        // Now, build the final SELECT that unions, groups, and ranks the results from the CTEs
        let query = if union_selects.is_empty() {
            // This is a fallback case; if no words matched, return nothing from the filtered table
            Query::select().expr(Expr::col(Asterisk)).from(base_from_table).take()
        } else {
            // Union all of the CTEs
            let union_query = union_selects
                .into_iter()
                .reduce(|mut acc, s| acc.union(UnionType::All, s).take())
                .expect("not empty");

            let u = "u";
            Query::select()
                .apply(|s| {
                    // Add the command fields to both the SELECT and GROUP BY clauses
                    for field in Command::VARIANTS {
                        let field = *field;
                        if field != Command::Table {
                            s.expr(Expr::col((u, field))).group_by_col((u, field));
                        }
                    }
                    // And the workspace flag
                    s.expr(Expr::col(IS_WORKSPACE)).group_by_col(IS_WORKSPACE);
                })
                .apply_if(rank_field_name, |s, rank_field| {
                    // Add the rank field if set
                    let like_word = format!("{} %", escape_like(words.first().unwrap()));
                    let like_term = format!("{}%", escape_like(words.first().unwrap()));
                    s.expr_as(
                        Expr::max(
                            Expr::col((u, rank_field)).mul(
                                Expr::case(
                                    Expr::col((u, Command::Cmd)).like(LikeExpr::new(like_word).escape('\\')),
                                    1.0 + tuning.auto.root,
                                )
                                .case(
                                    Expr::col((u, Command::Cmd)).like(LikeExpr::new(like_term).escape('\\')),
                                    1.0 + (0.5 * tuning.auto.root),
                                )
                                .finally(1.0),
                            ),
                        ),
                        rank_field,
                    );
                })
                .from_subquery(union_query, u)
                .take()
        };

        (ctes, query, has_rank)
    }
}

/// Helper function to escape FTS queries by wrapping them in double quotes and escaping inside double quotes
fn escape_fts(query: &str) -> String {
    format!("\"{}\"", query.replace("\"", "\"\""))
}

/// Helper function to escape like expressions by escaping `\`, '_' and `%` with `\`
fn escape_like(query: &str) -> String {
    query.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
}

/// Builds a sea-query expression for a single fuzzy term
fn expr_for_fuzzy_term(ft: &FuzzyTerm, table_alias: impl Iden + 'static) -> SimpleExpr {
    let col = (table_alias, Command::Cmd);
    let term_val = ft.term;
    match ft.kind {
        FuzzyTermKind::Fuzzy => {
            let pattern = format!(
                "%{}%",
                term_val
                    .chars()
                    .map(|c| match c {
                        '\\' | '%' | '_' => format!("\\{c}"),
                        _ => c.to_string(),
                    })
                    .join("%")
            );
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\'))
        }
        FuzzyTermKind::Exact => {
            let pattern = format!("%{}%", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\'))
        }
        FuzzyTermKind::ExactBoundary => {
            let regexp = format!(r"\b{}\b", regex::escape(term_val));
            Expr::col(col).regexp(regexp)
        }
        FuzzyTermKind::PrefixExact => {
            let pattern = format!("{}%", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\'))
        }
        FuzzyTermKind::SuffixExact => {
            let pattern = format!("%{}", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\'))
        }
        FuzzyTermKind::InverseExact => {
            let pattern = format!("%{}%", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\')).not()
        }
        FuzzyTermKind::InversePrefixExact => {
            let pattern = format!("{}%", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\')).not()
        }
        FuzzyTermKind::InverseSuffixExact => {
            let pattern = format!("%{}", escape_like(term_val));
            Expr::col(col).like(LikeExpr::new(pattern).escape('\\')).not()
        }
    }
}

/// Custom operator methods for building expressions
trait CustomExpr: ExprTrait {
    /// Express a custom sqlite `REGEXP` operator
    fn regexp<T>(self, right: T) -> Expr
    where
        T: Into<Expr>,
    {
        self.binary(CustomBinOper::Regexp, right)
    }
}
impl<T> CustomExpr for T where T: ExprTrait {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CustomBinOper {
    Regexp,
}
impl From<CustomBinOper> for BinOper {
    fn from(o: CustomBinOper) -> Self {
        Self::Custom(match o {
            CustomBinOper::Regexp => "REGEXP",
        })
    }
}

/// Function call helper.
#[derive(Debug, Clone)]
struct CustomFunc;
impl CustomFunc {
    /// Calls negated `BM25` function
    fn neg_bm25<T>(fts_col: T, cmd_weight: f64, description_weight: f64) -> FunctionCall
    where
        T: Into<Expr>,
    {
        Func::cust("-bm25").arg(fts_col).arg(cmd_weight).arg(description_weight)
    }

    /// Calls negated `CMD_TO_REGEX` function
    fn cmd_to_regex<T>(cmd_col: T) -> FunctionCall
    where
        T: Into<Expr>,
    {
        Func::cust("cmd_to_regex").arg(cmd_col)
    }

    /// Calls negated `LOG` function
    fn log<T>(expr: T) -> FunctionCall
    where
        T: Into<Expr>,
    {
        Func::cust("log").arg(expr)
    }

    /// Calls `ROW_NUMBER` function
    fn row_number() -> FunctionCall {
        Func::cust("row_number")
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Iden, strum::VariantArray)]
pub enum Command {
    Table,
    Rowid,
    Id,
    Category,
    Source,
    Alias,
    Cmd,
    FlatCmd,
    Description,
    FlatDescription,
    Tags,
    CreatedAt,
    UpdatedAt,
}

#[derive(Copy, Clone, Iden)]
pub enum CommandUsage {
    Table,
    CommandId,
    Path,
    UsageCount,
}

#[derive(Copy, Clone, Iden)]
pub enum CommandFuzzyFts {
    Table,
    Rowid,
}

#[derive(Copy, Clone, Iden)]
pub enum CommandFts {
    Table,
    Rowid,
}

#[derive(Copy, Clone, Iden)]
pub enum WorkspaceCommand {
    Table,
}

#[derive(Copy, Clone, Iden)]
pub enum WorkspaceCommandFuzzyFts {
    Table,
}

#[derive(Copy, Clone, Iden)]
pub enum WorkspaceCommandFts {
    Table,
}

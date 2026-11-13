//! Structured search: notes by words and details (`find_notes`), the tags and apps a search can
//! recognise (`search_facets`) and conversations with Cilica (`search_threads`). Owner: Core agent.
//!
//! - Words go through FTS5 (`notes_fts`); everything else is plain SQL on `notes` and its
//!   neighbours, so a search with only details ("meetings with Sam last week") needs no index.
//! - Dates are stored as RFC 3339 UTC text with a fixed format, so ranges compare as strings.
//!   A time of day is read in local time with the offset the caller passes.

use cilica_domain::{
    DateField, Message, MessagePart, NoteKind, NoteQuery, SearchFacets, SearchHit, SearchSource,
    Thread, ThreadHit, TrashScope,
};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::links::link_key;
use crate::notes::{MARK_START, MAX_SEARCH_RESULTS, excerpt_html, fts_query, summary_from_row};
use crate::{Db, StoreResult, codec};

/// Most tags, apps or providers returned by [`Db::search_facets`].
const MAX_FACETS: i64 = 500;

/// Characters of message text shown around a match in a thread hit.
const THREAD_EXCERPT_CHARS: usize = 90;

impl Db {
    // ── Notes ────────────────────────────────────────────────────────────

    /// Notes matching words and details. Contract.
    ///
    /// With words, ranked by bm25 (title highest) like [`Db::search_notes`]. With only details,
    /// newest first by the date the query is about. A query with neither returns nothing.
    pub async fn find_notes(&self, q: &NoteQuery, limit: u32) -> StoreResult<Vec<SearchHit>> {
        let match_expr = text_match(q);
        if match_expr.is_none() && !has_details(q) {
            return Ok(Vec::new());
        }
        if !q.tags.is_empty() || !q.links_to.is_empty() {
            self.refresh_link_index().await?;
        }
        let dated = q.after.is_some() || q.before.is_some() || q.time_of_day.is_some();

        // Columns
        let mut b: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT n.id, n.title, n.snippet, n.category_id, n.kind, n.meeting_app, n.position, n.created_at, n.updated_at, n.deleted_at, ",
        );
        if match_expr.is_some() {
            b.push(concat!(
                "snippet(notes_fts, 1, '\u{E000}', '\u{E001}', '…', 16) AS hit_title, ",
                "snippet(notes_fts, 2, '\u{E000}', '\u{E001}', '…', 16) AS hit_body, ",
                "snippet(notes_fts, 3, '\u{E000}', '\u{E001}', '…', 16) AS hit_transcript, "
            ));
        } else {
            b.push("NULL AS hit_title, NULL AS hit_body, NULL AS hit_transcript, ");
        }
        if dated {
            push_matched_at(&mut b, q);
        } else {
            b.push("NULL");
        }
        b.push(" AS matched_at ");

        // Source
        match &match_expr {
            Some(expr) => {
                b.push("FROM notes_fts JOIN notes n ON n.id = notes_fts.note_id WHERE notes_fts MATCH ");
                b.push_bind(expr.clone());
            }
            None => {
                b.push("FROM notes n WHERE 1 = 1");
            }
        }

        // Filters
        push_filters(&mut b, q);
        if dated {
            b.push(" AND ");
            push_date_condition(&mut b, q);
        }

        // Order
        if match_expr.is_some() {
            b.push(" ORDER BY bm25(notes_fts, 0.0, 10.0, 3.0, 1.0) ASC, n.updated_at DESC");
        } else {
            b.push(" ORDER BY COALESCE(matched_at, ");
            b.push(sort_column(q.date_field));
            b.push(") DESC");
        }
        b.push(" LIMIT ");
        b.push_bind(i64::from(limit.clamp(1, MAX_SEARCH_RESULTS)));

        let rows = b.build().fetch_all(self.pool()).await?;
        let mut hits = Vec::with_capacity(rows.len());
        for row in &rows {
            let note = summary_from_row(row)?;
            let matched_at = codec::parse_opt_ts(row.try_get("matched_at")?)?;
            if match_expr.is_none() {
                hits.push(SearchHit {
                    excerpt: excerpt_html(&note.snippet),
                    note,
                    source: SearchSource::Details,
                    at_ms: None,
                    matched_at,
                });
                continue;
            }
            let columns = [
                (
                    SearchSource::Title,
                    row.try_get::<Option<String>, _>("hit_title")?,
                ),
                (
                    SearchSource::Body,
                    row.try_get::<Option<String>, _>("hit_body")?,
                ),
                (
                    SearchSource::Transcript,
                    row.try_get::<Option<String>, _>("hit_transcript")?,
                ),
            ];
            let (source, raw) = columns
                .iter()
                .find(|(_, text)| text.as_deref().is_some_and(|t| t.contains(MARK_START)))
                .map(|(source, text)| (*source, text.clone().unwrap_or_default()))
                // A match the snippet could not show (should not happen): fall back to the title.
                .unwrap_or((SearchSource::Title, note.title.clone()));
            let at_ms = match source {
                SearchSource::Transcript => self.segment_at_for_match(note.id, &raw).await?,
                _ => None,
            };
            hits.push(SearchHit {
                excerpt: excerpt_html(&raw),
                note,
                source,
                at_ms,
                matched_at,
            });
        }
        Ok(hits)
    }

    // ── Facets ───────────────────────────────────────────────────────────

    /// The tags on notes not in Trash (most used first), the apps meetings were recorded in and
    /// the apps notes were imported from. Contract.
    pub async fn search_facets(&self) -> StoreResult<SearchFacets> {
        self.refresh_link_index().await?;
        let tags = sqlx::query_scalar::<_, String>(
            "SELECT MIN(l.target) FROM note_links l JOIN notes n ON n.id = l.note_id \
             WHERE l.kind = 'tag' AND n.deleted_at IS NULL GROUP BY l.target_key ORDER BY COUNT(*) DESC, l.target_key LIMIT ?",
        )
        .bind(MAX_FACETS)
        .fetch_all(self.pool())
        .await?;
        let meeting_apps = sqlx::query_scalar::<_, String>(
            "SELECT meeting_app FROM notes WHERE meeting_app IS NOT NULL AND meeting_app != '' AND deleted_at IS NULL \
             GROUP BY lower(meeting_app) ORDER BY COUNT(*) DESC LIMIT ?",
        )
        .bind(MAX_FACETS)
        .fetch_all(self.pool())
        .await?;
        let import_providers = sqlx::query_scalar::<_, String>(
            "SELECT import_provider FROM notes WHERE import_provider IS NOT NULL AND deleted_at IS NULL \
             GROUP BY import_provider ORDER BY COUNT(*) DESC LIMIT ?",
        )
        .bind(MAX_FACETS)
        .fetch_all(self.pool())
        .await?;
        Ok(SearchFacets {
            tags,
            meeting_apps,
            import_providers,
        })
    }

    // ── Threads ──────────────────────────────────────────────────────────

    /// Conversations with Cilica whose title or messages contain every word, most recent first.
    /// The excerpt is from the first message (or the title) with the first word in it. Contract.
    pub async fn search_threads(&self, query: &str, limit: u32) -> StoreResult<Vec<ThreadHit>> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(str::to_lowercase)
            .take(16)
            .collect();
        let Some(first) = words.first() else {
            return Ok(Vec::new());
        };

        // Candidates: the first word in the title or anywhere in a message.
        let pattern = format!("%{}%", like_escape(first));
        let candidates = sqlx::query(
            "SELECT id, title, created_at, updated_at FROM threads t WHERE t.title LIKE ?1 ESCAPE '\\' \
             OR EXISTS (SELECT 1 FROM messages m WHERE m.thread_id = t.id AND m.parts_json LIKE ?1 ESCAPE '\\') \
             ORDER BY t.updated_at DESC",
        )
        .bind(&pattern)
        .fetch_all(self.pool())
        .await?;

        // Every word, anywhere in the title or the messages' text.
        let mut hits = Vec::new();
        for row in &candidates {
            if hits.len() >= limit as usize {
                break;
            }
            let thread = Thread {
                id: codec::parse_id(&row.try_get::<String, _>("id")?)?,
                title: row.try_get("title")?,
                created_at: codec::parse_ts(&row.try_get::<String, _>("created_at")?)?,
                updated_at: codec::parse_ts(&row.try_get::<String, _>("updated_at")?)?,
            };
            let texts: Vec<String> = self
                .list_messages(thread.id)
                .await?
                .into_iter()
                .flat_map(|(message, _)| message_texts(&message))
                .collect();
            let all = format!("{}\n{}", thread.title, texts.join("\n")).to_lowercase();
            if !words.iter().all(|w| all.contains(w.as_str())) {
                continue;
            }
            let source = texts
                .iter()
                .find(|t| t.to_lowercase().contains(first.as_str()))
                .map_or(thread.title.as_str(), String::as_str);
            let excerpt = mark_excerpt(source, &words);
            hits.push(ThreadHit { thread, excerpt });
        }
        Ok(hits)
    }
}

// ── Query building ──────────────────────────────────────────────────────────

/// The FTS5 expression for the query's words and phrases, or `None` when it has neither.
fn text_match(q: &NoteQuery) -> Option<String> {
    let mut parts: Vec<String> = q.phrases.iter().filter_map(|p| fts_phrase(p)).collect();
    if let Some(words) = fts_query(&q.text) {
        parts.push(words);
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// A quoted FTS5 phrase from the words of `text`, or `None` when it has no words.
fn fts_phrase(text: &str) -> Option<String> {
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(16)
        .collect();
    (!words.is_empty()).then(|| format!("\"{}\"", words.join(" ")))
}

/// Whether the query asks for anything besides words (a date, a person, Trash…).
fn has_details(q: &NoteQuery) -> bool {
    q.after.is_some()
        || q.before.is_some()
        || q.time_of_day.is_some()
        || q.kind.is_some()
        || !q.category_ids.is_empty()
        || q.uncategorized
        || !q.person_ids.is_empty()
        || !q.tags.is_empty()
        || !q.links_to.is_empty()
        || q.meeting_app.is_some()
        || q.min_duration_ms.is_some()
        || q.max_duration_ms.is_some()
        || q.has_recording.is_some()
        || q.has_quotes.is_some()
        || q.import_provider.is_some()
        || q.trash == TrashScope::Only
        || !q.exclude.is_empty()
        || q.date_field == DateField::Deleted
}

/// The date columns of `notes n` a date field looks at, in order of preference.
fn date_columns(field: DateField) -> &'static [&'static str] {
    match field {
        DateField::Any => &["COALESCE(n.started_at, n.created_at)", "n.updated_at"],
        DateField::Created => &["n.created_at"],
        DateField::Edited => &["n.updated_at"],
        DateField::Met => &["n.started_at"],
        DateField::Deleted => &["n.deleted_at"],
    }
}

/// What a details-only search sorts by, newest first.
fn sort_column(field: DateField) -> &'static str {
    match field {
        DateField::Any | DateField::Met => "COALESCE(n.started_at, n.updated_at)",
        DateField::Created => "n.created_at",
        DateField::Edited => "n.updated_at",
        DateField::Deleted => "COALESCE(n.deleted_at, n.updated_at)",
    }
}

/// `(<column> in the range and the time of day)`.
fn push_in_range(b: &mut QueryBuilder<Sqlite>, q: &NoteQuery, column: &str) {
    b.push("(").push(column).push(" IS NOT NULL");
    if let Some(after) = q.after {
        b.push(" AND ")
            .push(column)
            .push(" >= ")
            .push_bind(codec::ts(after));
    }
    if let Some(before) = q.before {
        b.push(" AND ")
            .push(column)
            .push(" < ")
            .push_bind(codec::ts(before));
    }
    if let Some(window) = q.time_of_day {
        let shift = format!("{:+} minutes", q.utc_offset_minutes);
        let minute = |b: &mut QueryBuilder<Sqlite>| {
            b.push("(CAST(strftime('%H', ")
                .push(column)
                .push(", ")
                .push_bind(shift.clone())
                .push(") AS INTEGER) * 60 + CAST(strftime('%M', ");
            b.push(column)
                .push(", ")
                .push_bind(shift.clone())
                .push(") AS INTEGER))");
        };
        let joiner = if window.start <= window.end {
            " AND "
        } else {
            " OR "
        };
        b.push(" AND (");
        minute(b);
        b.push(" >= ")
            .push_bind(i64::from(window.start))
            .push(joiner);
        minute(b);
        b.push(" < ").push_bind(i64::from(window.end)).push(")");
    }
    b.push(")");
}

/// The note matches the query's date range and time of day on the field it asks about.
fn push_date_condition(b: &mut QueryBuilder<Sqlite>, q: &NoteQuery) {
    b.push("(");
    for (i, column) in date_columns(q.date_field).iter().enumerate() {
        if i > 0 {
            b.push(" OR ");
        }
        push_in_range(b, q, column);
    }
    if q.date_field == DateField::Edited {
        b.push(" OR EXISTS (SELECT 1 FROM note_versions v WHERE v.note_id = n.id AND ");
        push_in_range(b, q, "v.created_at");
        b.push(")");
    }
    b.push(")");
}

/// The first of the field's dates that matched, for the hit's `matched_at`.
fn push_matched_at(b: &mut QueryBuilder<Sqlite>, q: &NoteQuery) {
    b.push("CASE");
    for column in date_columns(q.date_field) {
        b.push(" WHEN ");
        push_in_range(b, q, column);
        b.push(" THEN ").push(*column);
    }
    if q.date_field == DateField::Edited {
        b.push(" ELSE (SELECT MAX(v.created_at) FROM note_versions v WHERE v.note_id = n.id AND ");
        push_in_range(b, q, "v.created_at");
        b.push(")");
    }
    b.push(" END");
}

/// Every filter besides words and dates.
fn push_filters(b: &mut QueryBuilder<Sqlite>, q: &NoteQuery) {
    // Trash
    match q.trash {
        TrashScope::Exclude if q.date_field != DateField::Deleted => {
            b.push(" AND n.deleted_at IS NULL");
        }
        TrashScope::Only => {
            b.push(" AND n.deleted_at IS NOT NULL");
        }
        _ => {}
    }

    // Kind, category
    if let Some(kind) = q.kind {
        b.push(" AND n.kind = ")
            .push_bind(if kind == NoteKind::Meeting {
                "meeting"
            } else {
                "manual"
            });
    }
    if !q.category_ids.is_empty() || q.uncategorized {
        b.push(" AND (");
        if q.uncategorized {
            b.push("n.category_id IS NULL");
        }
        if !q.category_ids.is_empty() {
            if q.uncategorized {
                b.push(" OR ");
            }
            b.push("n.category_id IN (");
            let mut ids = b.separated(", ");
            for id in &q.category_ids {
                ids.push_bind(codec::id(*id));
            }
            b.push(")");
        }
        b.push(")");
    }

    // People, tags, links
    for person in &q.person_ids {
        b.push(" AND EXISTS (SELECT 1 FROM speakers s WHERE s.note_id = n.id AND s.person_id = ")
            .push_bind(codec::id(*person))
            .push(")");
    }
    for tag in &q.tags {
        let key = link_key(tag.trim().trim_start_matches('#'));
        b.push(" AND EXISTS (SELECT 1 FROM note_links l WHERE l.note_id = n.id AND l.kind = 'tag' AND l.target_key = ").push_bind(key).push(")");
    }
    for title in &q.links_to {
        b.push(" AND EXISTS (SELECT 1 FROM note_links l WHERE l.note_id = n.id AND l.kind IN ('link', 'embed') AND l.target_key = ")
            .push_bind(link_key(title))
            .push(")");
    }

    // Meeting details
    if let Some(app) = q
        .meeting_app
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
    {
        b.push(" AND instr(lower(n.meeting_app), lower(")
            .push_bind(app.to_string())
            .push(")) > 0");
    }
    const DURATION: &str =
        "COALESCE(n.duration_ms, (julianday(n.ended_at) - julianday(n.started_at)) * 86400000.0)";
    if let Some(min) = q.min_duration_ms {
        b.push(" AND ")
            .push(DURATION)
            .push(" >= ")
            .push_bind(i64::from(min));
    }
    if let Some(max) = q.max_duration_ms {
        b.push(" AND ")
            .push(DURATION)
            .push(" <= ")
            .push_bind(i64::from(max));
    }
    match q.has_recording {
        Some(true) => {
            b.push(" AND n.audio_path IS NOT NULL");
        }
        Some(false) => {
            b.push(" AND n.audio_path IS NULL");
        }
        None => {}
    }
    match q.has_quotes {
        Some(true) => {
            b.push(" AND COALESCE(n.quotes_json, '') NOT IN ('', '[]', 'null')");
        }
        Some(false) => {
            b.push(" AND COALESCE(n.quotes_json, '') IN ('', '[]', 'null')");
        }
        None => {}
    }
    if let Some(provider) = &q.import_provider {
        b.push(" AND n.import_provider = ")
            .push_bind(provider.clone());
    }

    // Words that must not appear
    for word in &q.exclude {
        if let Some(phrase) = fts_phrase(word) {
            b.push(" AND n.id NOT IN (SELECT note_id FROM notes_fts WHERE notes_fts MATCH ")
                .push_bind(phrase)
                .push(")");
        }
    }
}

// ── Threads ─────────────────────────────────────────────────────────────────

/// The text a person reads in a message: its text parts.
fn message_texts(message: &Message) -> Vec<String> {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Escape `%`, `_` and `\` for a `LIKE … ESCAPE '\'` pattern.
fn like_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// About [`THREAD_EXCERPT_CHARS`] characters of `text` around the first word, every word wrapped
/// in `<mark>` and the rest HTML-escaped.
fn mark_excerpt(text: &str, words: &[String]) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    let lower: Vec<char> = flat.to_lowercase().chars().collect();
    // Lowercasing can change length for a few scripts; fall back to no marks then.
    let same_len = lower.len() == chars.len();
    let find = |needle: &str, from: usize| -> Option<usize> {
        let needle: Vec<char> = needle.chars().collect();
        if needle.is_empty() || !same_len {
            return None;
        }
        (from..lower.len().saturating_sub(needle.len() - 1))
            .find(|&i| lower[i..i + needle.len()] == needle[..])
    };

    // Window around the first match
    let first = words.first().and_then(|w| find(w, 0)).unwrap_or(0);
    let start = first.saturating_sub(30);
    let end = (start + THREAD_EXCERPT_CHARS).min(chars.len());
    let mut marked = vec![false; chars.len()];
    for word in words {
        let len = word.chars().count();
        let mut from = start;
        while let Some(i) = find(word, from) {
            if i >= end {
                break;
            }
            for m in marked.iter_mut().skip(i).take(len) {
                *m = true;
            }
            from = i + len.max(1);
        }
    }

    // Raw text with the private markers, then escaped like FTS snippets
    let mut raw = String::new();
    if start > 0 {
        raw.push('…');
    }
    let mut open = false;
    for i in start..end {
        if marked[i] && !open {
            raw.push(MARK_START);
            open = true;
        } else if !marked[i] && open {
            raw.push('\u{E001}');
            open = false;
        }
        raw.push(chars[i]);
    }
    if open {
        raw.push('\u{E001}');
    }
    if end < chars.len() {
        raw.push('…');
    }
    excerpt_html(&raw)
}

#[cfg(test)]
mod tests {
    use cilica_domain::chrono::{TimeDelta, Utc};
    use cilica_domain::{Id, MessagePart, NoteKind, Role, TimeOfDay, new_id, now};

    use super::*;
    use crate::NewNote;

    async fn db() -> Db {
        Db::open_in_memory().await.unwrap()
    }

    async fn note(db: &Db, title: &str, body: &str) -> Id {
        db.create_note(NewNote {
            title: title.into(),
            body_md: body.into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
    }

    /// Set raw columns the public API does not write directly.
    async fn set(db: &Db, id: Id, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE notes SET {sql} WHERE id = ?"
        )))
        .bind(codec::id(id))
        .execute(db.pool())
        .await
        .unwrap();
    }

    async fn ids(db: &Db, q: NoteQuery) -> Vec<Id> {
        db.find_notes(&q, 50)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.note.id)
            .collect()
    }

    fn at(days_ago: i64, hour: u32) -> String {
        let day = (Utc::now() - TimeDelta::days(days_ago)).date_naive();
        codec::ts(day.and_hms_opt(hour, 0, 0).unwrap().and_utc())
    }

    #[tokio::test]
    async fn nothing_asked_finds_nothing() {
        let db = db().await;
        note(&db, "Plan", "body").await;
        assert!(ids(&db, NoteQuery::default()).await.is_empty());
        assert!(
            ids(
                &db,
                NoteQuery {
                    trash: TrashScope::Include,
                    ..Default::default()
                }
            )
            .await
            .is_empty()
        );
    }

    #[tokio::test]
    async fn words_still_rank_like_search_notes() {
        let db = db().await;
        let a = note(&db, "Pricing", "plans").await;
        let b = note(&db, "Other", "about pricing").await;
        let hits = db
            .find_notes(
                &NoteQuery {
                    text: "pricing".into(),
                    ..Default::default()
                },
                10,
            )
            .await
            .unwrap();
        assert_eq!(
            hits.iter().map(|h| h.note.id).collect::<Vec<_>>(),
            vec![a, b]
        );
        assert_eq!(hits[0].source, SearchSource::Title);
        assert!(hits[0].matched_at.is_none());
    }

    #[tokio::test]
    async fn phrases_and_excluded_words() {
        let db = db().await;
        let a = note(&db, "A", "the launch plan is ready").await;
        let b = note(&db, "B", "plan the launch later").await;
        let phrase = NoteQuery {
            phrases: vec!["launch plan".into()],
            ..Default::default()
        };
        assert_eq!(ids(&db, phrase).await, vec![a]);
        let excluded = NoteQuery {
            text: "launch".into(),
            exclude: vec!["ready".into()],
            ..Default::default()
        };
        assert_eq!(ids(&db, excluded).await, vec![b]);
    }

    #[tokio::test]
    async fn date_range_on_any_field_reports_the_matching_date() {
        let db = db().await;
        let old = note(&db, "Old", "").await;
        let met = note(&db, "Met yesterday", "").await;
        set(
            &db,
            old,
            &format!("created_at = '{0}', updated_at = '{0}'", at(10, 9)),
        )
        .await;
        set(
            &db,
            met,
            &format!(
                "kind = 'meeting', started_at = '{}', created_at = '{0}', updated_at = '{0}'",
                at(1, 15)
            ),
        )
        .await;
        let fresh = note(&db, "Edited today", "").await;

        let yesterday = NoteQuery {
            after: Some(
                (Utc::now() - TimeDelta::days(1))
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            before: Some(
                Utc::now()
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            ..Default::default()
        };
        let hits = db.find_notes(&yesterday, 10).await.unwrap();
        assert_eq!(
            hits.iter().map(|h| h.note.id).collect::<Vec<_>>(),
            vec![met]
        );
        assert_eq!(hits[0].source, SearchSource::Details);
        assert_eq!(hits[0].matched_at.map(codec::ts), Some(at(1, 15)));

        let since_week = NoteQuery {
            after: Some(Utc::now() - TimeDelta::days(7)),
            ..Default::default()
        };
        let found = ids(&db, since_week).await;
        assert!(found.contains(&met) && found.contains(&fresh) && !found.contains(&old));
    }

    #[tokio::test]
    async fn edited_matches_saved_versions() {
        let db = db().await;
        let id = note(&db, "Doc", "v1").await;
        db.set_note_body(id, "v2", cilica_domain::Author::Ai, None)
            .await
            .unwrap();
        let ten_days = Utc::now() - TimeDelta::days(10);
        set(&db, id, &format!("updated_at = '{}'", codec::ts(ten_days))).await;
        sqlx::query("UPDATE note_versions SET created_at = ? WHERE note_id = ?")
            .bind(at(3, 12))
            .bind(codec::id(id))
            .execute(db.pool())
            .await
            .unwrap();
        let q = |field| NoteQuery {
            after: Some(Utc::now() - TimeDelta::days(4)),
            before: Some(Utc::now() - TimeDelta::days(2)),
            date_field: field,
            ..Default::default()
        };
        let hits = db.find_notes(&q(DateField::Edited), 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_at.map(codec::ts), Some(at(3, 12)));
        assert!(ids(&db, q(DateField::Created)).await.is_empty());
    }

    #[tokio::test]
    async fn time_of_day_reads_local_time() {
        let db = db().await;
        let morning = note(&db, "Morning", "").await;
        let evening = note(&db, "Evening", "").await;
        set(
            &db,
            morning,
            &format!("created_at = '{0}', updated_at = '{0}'", at(2, 8)),
        )
        .await;
        set(
            &db,
            evening,
            &format!("created_at = '{0}', updated_at = '{0}'", at(2, 19)),
        )
        .await;
        let window = |start, end, offset| NoteQuery {
            time_of_day: Some(TimeOfDay { start, end }),
            utc_offset_minutes: offset,
            date_field: DateField::Created,
            ..Default::default()
        };
        assert_eq!(ids(&db, window(8 * 60, 9 * 60, 0)).await, vec![morning]);
        // 19:00 UTC is 21:00 at UTC+2.
        assert_eq!(ids(&db, window(21 * 60, 22 * 60, 120)).await, vec![evening]);
        // Across midnight: 18:00 to 02:00.
        assert_eq!(ids(&db, window(18 * 60, 2 * 60, 0)).await, vec![evening]);
    }

    #[tokio::test]
    async fn meeting_details() {
        let db = db().await;
        let zoom = note(&db, "Zoom call", "").await;
        let meet = note(&db, "Meet call", "").await;
        let manual = note(&db, "Plain", "").await;
        set(&db, zoom, "kind = 'meeting', meeting_app = 'Zoom', duration_ms = 3600000, audio_path = 'a.opus', quotes_json = '[{\"text\":\"hi\"}]'").await;
        set(&db, meet, "kind = 'meeting', meeting_app = 'Google Meet', duration_ms = 600000, quotes_json = '[]'").await;

        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    kind: Some(NoteKind::Manual),
                    ..Default::default()
                }
            )
            .await,
            vec![manual]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    meeting_app: Some("zoom".into()),
                    ..Default::default()
                }
            )
            .await,
            vec![zoom]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    meeting_app: Some("meet".into()),
                    ..Default::default()
                }
            )
            .await,
            vec![meet]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    min_duration_ms: Some(30 * 60_000),
                    ..Default::default()
                }
            )
            .await,
            vec![zoom]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    max_duration_ms: Some(15 * 60_000),
                    ..Default::default()
                }
            )
            .await,
            vec![meet]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    has_recording: Some(true),
                    ..Default::default()
                }
            )
            .await,
            vec![zoom]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    has_quotes: Some(true),
                    ..Default::default()
                }
            )
            .await,
            vec![zoom]
        );
        let no_quotes = ids(
            &db,
            NoteQuery {
                has_quotes: Some(false),
                kind: Some(NoteKind::Meeting),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(no_quotes, vec![meet]);
    }

    #[tokio::test]
    async fn people_categories_and_import_source() {
        let db = db().await;
        let cat = db.create_category("Work").await.unwrap();
        let a = db
            .create_note(NewNote {
                title: "A".into(),
                category_id: Some(cat.id),
                ..Default::default()
            })
            .await
            .unwrap()
            .id;
        let b = note(&db, "B", "").await;
        set(&db, b, "import_provider = 'notion'").await;

        let sam = new_id();
        sqlx::query("INSERT INTO people (id, name, created_at) VALUES (?, 'Sam', ?)")
            .bind(codec::id(sam))
            .bind(codec::ts(now()))
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO speakers (id, note_id, label, person_id) VALUES (?, ?, 'Person 1', ?)",
        )
        .bind(codec::id(new_id()))
        .bind(codec::id(b))
        .bind(codec::id(sam))
        .execute(db.pool())
        .await
        .unwrap();

        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    category_ids: vec![cat.id],
                    ..Default::default()
                }
            )
            .await,
            vec![a]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    uncategorized: true,
                    ..Default::default()
                }
            )
            .await,
            vec![b]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    person_ids: vec![sam],
                    ..Default::default()
                }
            )
            .await,
            vec![b]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    import_provider: Some("notion".into()),
                    ..Default::default()
                }
            )
            .await,
            vec![b]
        );
    }

    #[tokio::test]
    async fn tags_and_links_read_the_link_index() {
        let db = db().await;
        let a = note(&db, "A", "about #Pricing and [[Roadmap]]").await;
        note(&db, "B", "#other").await;
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    tags: vec!["#pricing".into()],
                    ..Default::default()
                }
            )
            .await,
            vec![a]
        );
        assert_eq!(
            ids(
                &db,
                NoteQuery {
                    links_to: vec!["roadmap".into()],
                    ..Default::default()
                }
            )
            .await,
            vec![a]
        );
        let facets = db.search_facets().await.unwrap();
        assert!(
            facets.tags.contains(&"Pricing".to_string())
                && facets.tags.contains(&"other".to_string())
        );
    }

    #[tokio::test]
    async fn trash_scope() {
        let db = db().await;
        let live = note(&db, "Live plan", "").await;
        let gone = note(&db, "Gone plan", "").await;
        db.trash_note(gone).await.unwrap();
        let words = |trash| NoteQuery {
            text: "plan".into(),
            trash,
            ..Default::default()
        };
        assert_eq!(ids(&db, words(TrashScope::Exclude)).await, vec![live]);
        assert_eq!(ids(&db, words(TrashScope::Only)).await, vec![gone]);
        assert_eq!(ids(&db, words(TrashScope::Include)).await.len(), 2);
        let deleted_today = NoteQuery {
            date_field: DateField::Deleted,
            after: Some(Utc::now() - TimeDelta::hours(1)),
            ..Default::default()
        };
        assert_eq!(ids(&db, deleted_today).await, vec![gone]);
    }

    #[tokio::test]
    async fn facets_list_apps_and_providers() {
        let db = db().await;
        let a = note(&db, "A", "").await;
        let b = note(&db, "B", "").await;
        set(&db, a, "meeting_app = 'Zoom', import_provider = 'bear'").await;
        set(&db, b, "meeting_app = 'zoom'").await;
        let facets = db.search_facets().await.unwrap();
        assert_eq!(facets.meeting_apps.len(), 1);
        assert_eq!(facets.import_providers, vec!["bear".to_string()]);
    }

    #[tokio::test]
    async fn threads_match_every_word_in_title_or_text() {
        let db = db().await;
        let thread = db.create_thread("Launch questions").await.unwrap();
        let message = Message {
            id: new_id(),
            thread_id: thread.id,
            turn_id: new_id(),
            role: Role::User,
            parts: vec![MessagePart::Text {
                text: "What did Sam say about the pricing page?".into(),
            }],
            created_at: now(),
        };
        db.save_message(&message, None).await.unwrap();
        db.create_thread("Unrelated").await.unwrap();

        let hits = db.search_threads("pricing launch", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].thread.id, thread.id);
        assert!(
            hits[0].excerpt.contains("<mark>pricing</mark>"),
            "{}",
            hits[0].excerpt
        );
        assert!(
            db.search_threads("pricing nowhere", 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(db.search_threads("  ", 10).await.unwrap().is_empty());
        assert!(db.search_threads("100%", 10).await.unwrap().is_empty());
    }

    #[test]
    fn excerpt_marks_every_word_and_escapes() {
        let words = vec!["plan".to_string(), "<b>".to_string()];
        assert_eq!(
            mark_excerpt("A <b> plan", &words),
            "A <mark>&lt;b&gt;</mark> <mark>plan</mark>"
        );
    }
}

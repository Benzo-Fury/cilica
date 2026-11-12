//! Account imports with a personal API key: Granola, Fireflies, Fathom, Otter, tl;dv, Circleback,
//! MeetGeek, Jamie and Limitless. Shapes follow each vendor's published API reference
//! (checked 12 Nov 2026); fields are read leniently so a renamed or missing one never fails a note.
//! Every provider also accepts its file exports through the default file reader.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use margin_domain::chrono::TimeDelta;
use margin_domain::{ImportFidelity, ImportInput, Timestamp};
use serde_json::{Value, json};

use super::{Cx, Loader, Provider, RemoteEntry, Scan, scan_paste, scan_paths};
use crate::ImportError;
use crate::formats::transcript::{finish, parse_text, speaker_ref};
use crate::http::{Http, WaitNotice, encode};
use crate::model::{ImportedNote, ImportedTranscript, SpeakerRef, Turn};
use crate::text::{parse_clock, parse_date, secs_to_ms};

// ── Reading JSON leniently ────────────────────────────────────────────────

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(|x| match x {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
    .filter(|s| !s.is_empty())
}

fn date(v: &Value, ptr: &str) -> Option<Timestamp> {
    s(v, ptr).and_then(|d| parse_date(&d))
}

fn arr<'a>(v: &'a Value, ptr: &str) -> &'a [Value] {
    v.pointer(ptr).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn num(v: &Value, ptr: &str) -> Option<f64> {
    v.pointer(ptr).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())))
}

fn names(items: &[Value], keys: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if let Some(n) = keys.iter().find_map(|k| s(item, k)).filter(|n| !out.contains(n)) {
            out.push(n);
        }
    }
    out
}

/// "Zoom" from a join link.
fn meeting_app(url: Option<String>) -> Option<String> {
    let url = url?.to_lowercase();
    Some(
        if url.contains("zoom.") {
            "Zoom"
        } else if url.contains("meet.google") {
            "Google Meet"
        } else if url.contains("teams.") {
            "Microsoft Teams"
        } else if url.contains("webex") {
            "Webex"
        } else {
            return None;
        }
        .to_string(),
    )
}

fn duration_between(a: Option<Timestamp>, b: Option<Timestamp>) -> Option<u32> {
    let d = (b? - a?).num_milliseconds();
    (d > 0).then(|| d.min(u32::MAX as i64) as u32)
}

/// All fidelity flags a meeting account usually carries.
const MEETINGS: ImportFidelity = ImportFidelity { summary: true, my_notes: false, transcript: true, speakers: true, timestamps: true, attachments: false };

/// The default file and paste handling shared by every account provider.
async fn files_or_paste(input: &ImportInput) -> Option<Result<Scan, ImportError>> {
    match input {
        ImportInput::Paths { paths } => Some(scan_paths(paths).await),
        ImportInput::Paste { text, title } => Some(Ok(scan_paste(text, title.as_deref()))),
        _ => None,
    }
}

fn need_key(cx: &Cx<'_>, name: &str) -> Result<String, ImportError> {
    cx.key.clone().ok_or_else(|| ImportError::invalid(format!("Paste your {name} API key first.")))
}

/// Pages through a cursor list, stopping when a page is empty or the cursor repeats.
const MAX_PAGES: usize = 2_000;

// ── Granola ──────────────────────────────────────────────────────────────

pub struct Granola;
const GRANOLA: &str = "https://public-api.granola.ai/v1";

fn granola_http(key: &str, notice: Option<WaitNotice>) -> Http {
    // 5 requests a second sustained.
    Http::new("Granola", Duration::from_millis(220)).bearer(key).with_notice(notice)
}

#[async_trait]
impl Provider for Granola {
    fn id(&self) -> &'static str {
        "granola"
    }
    fn name(&self) -> &'static str {
        "Granola"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Granola.app"]
    }

    async fn detect(&self, paths: &crate::KnownPaths) -> super::Detect {
        let installed = super::app_installed(paths, self.apps());
        super::Detect {
            app_installed: installed,
            note: installed.then(|| "Granola encrypts the notes it keeps on this Mac, so Margin reads them through your API key or a CSV export instead.".to_string()),
            ..Default::default()
        }
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Granola")?;
        let http = granola_http(&key, cx.notice.clone());
        let mut remote = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let url = format!("{GRANOLA}/notes?page_size=30{}", cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default());
            let page = http.get(&url).await?.body;
            for n in arr(&page, "/notes") {
                if s(n, "/deleted_at").is_some() {
                    continue;
                }
                let Some(id) = s(n, "/id") else { continue };
                remote.push(RemoteEntry { external_id: id, title: s(n, "/title").unwrap_or_else(|| "Granola meeting".into()), date: date(n, "/created_at") });
            }
            cursor = s(&page, "/cursor");
            if page.pointer("/hasMore").and_then(Value::as_bool) != Some(true) || cursor.is_none() {
                break;
            }
        }
        Ok(Scan {
            origin: "your Granola account".into(),
            remote,
            loader: Some(Arc::new(GranolaLoader { key })),
            declared: Some(ImportFidelity { my_notes: true, ..MEETINGS }),
            ..Default::default()
        })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let page = granola_http(key, None).get(&format!("{GRANOLA}/notes?page_size=1")).await?.body;
        Ok(arr(&page, "/notes").first().and_then(|n| s(n, "/owner/email")).map(|e| format!("Connected as {e}")))
    }
}

struct GranolaLoader {
    key: String,
}

#[async_trait]
impl Loader for GranolaLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let http = granola_http(&self.key, notice);
        let id = &entry.external_id;
        let note = match http.get(&format!("{GRANOLA}/notes/{}?include=transcript", encode(id))).await {
            Ok(r) => r.body,
            Err(e) if e.too_large => http.get(&format!("{GRANOLA}/notes/{}", encode(id))).await?.body,
            Err(e) => return Err(e),
        };
        let mut items: Vec<Value> = arr(&note, "/transcript").to_vec();
        if note.get("transcript").is_none_or(Value::is_null) {
            // Too large for one answer: page through the transcript endpoint.
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_PAGES {
                let url = format!("{GRANOLA}/notes/{}/transcript?page_size=100{}", encode(id), cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default());
                let Ok(page) = http.get(&url).await.map(|r| r.body) else { break };
                items.extend(arr(&page, "/transcript").iter().cloned());
                cursor = s(&page, "/cursor");
                if page.pointer("/hasMore").and_then(Value::as_bool) != Some(true) || cursor.is_none() {
                    break;
                }
            }
        }
        let first = items.iter().filter_map(|t| date(t, "/start_time")).min();
        let scheduled = date(&note, "/calendar_event/scheduled_start_time");
        let zero = first.or(scheduled);
        let offset = |t: Option<Timestamp>| -> Option<u32> { zero.zip(t).map(|(z, t)| (t - z).num_milliseconds().clamp(0, u32::MAX as i64) as u32) };
        let turns: Vec<Turn> = items
            .iter()
            .filter_map(|t| {
                let text = s(t, "/text")?;
                let speaker = if s(t, "/speaker/attribution").as_deref() == Some("me") {
                    SpeakerRef::Me
                } else if let Some(name) = s(t, "/speaker/name") {
                    speaker_ref(&name)
                } else if s(t, "/speaker/source").as_deref() == Some("microphone") {
                    SpeakerRef::Me
                } else {
                    SpeakerRef::Them
                };
                Some(Turn { speaker, start_ms: offset(date(t, "/start_time")), end_ms: offset(date(t, "/end_time")), text })
            })
            .collect();
        let started = first.or(scheduled);
        let ended = items.iter().filter_map(|t| date(t, "/end_time")).max().or_else(|| date(&note, "/calendar_event/scheduled_end_time"));
        Ok(ImportedNote {
            external_id: id.clone(),
            title: s(&note, "/title").or_else(|| s(&note, "/calendar_event/event_title")).unwrap_or_else(|| entry.title.clone()),
            created_at: date(&note, "/created_at"),
            updated_at: date(&note, "/updated_at"),
            started_at: started,
            ended_at: ended,
            duration_ms: duration_between(started, ended),
            attendees: names(arr(&note, "/attendees"), &["/name", "/email"]),
            summary_md: s(&note, "/summary_markdown").or_else(|| s(&note, "/summary_text")),
            notes_md: s(&note, "/private_notes_markdown").or_else(|| s(&note, "/private_notes_text")),
            folder: arr(&note, "/folder_membership").first().and_then(|f| s(f, "/name")).into_iter().collect(),
            transcript: (!turns.is_empty()).then(|| ImportedTranscript::new(turns)),
            meeting: true,
            ..Default::default()
        })
    }
}

// ── Fireflies ────────────────────────────────────────────────────────────

pub struct Fireflies;
const FIREFLIES: &str = "https://api.fireflies.ai/graphql";
const FIREFLIES_QUERY: &str = "query Transcripts($limit: Int, $skip: Int) { transcripts(limit: $limit, skip: $skip) { \
    id title date duration organizer_email meeting_link \
    meeting_attendees { displayName name email } \
    sentences { speaker_name text start_time end_time } \
    summary { overview short_summary notes action_items shorthand_bullet } } }";

fn fireflies_http(key: &str, notice: Option<WaitNotice>) -> Http {
    // Free plans allow 30 requests a minute.
    Http::new("Fireflies", Duration::from_millis(2_100)).bearer(key).with_notice(notice)
}

#[async_trait]
impl Provider for Fireflies {
    fn id(&self) -> &'static str {
        "fireflies"
    }
    fn name(&self) -> &'static str {
        "Fireflies"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Fireflies.app", "Fireflies.ai.app"]
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Fireflies")?;
        let http = fireflies_http(&key, cx.notice.clone());
        let mut notes = Vec::new();
        let mut skip = 0;
        for _ in 0..MAX_PAGES {
            let reply = http.post_json(FIREFLIES, &json!({ "query": FIREFLIES_QUERY, "variables": { "limit": 50, "skip": skip } })).await?.body;
            if let Some(err) = s(&reply, "/errors/0/message") {
                return Err(ImportError::upstream(format!("Fireflies said: {err}")));
            }
            let page = arr(&reply, "/data/transcripts");
            notes.extend(page.iter().map(fireflies_note));
            if page.len() < 50 {
                break;
            }
            skip += 50;
        }
        Ok(Scan { origin: "your Fireflies account".into(), notes, ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let reply = fireflies_http(key, None).post_json(FIREFLIES, &json!({ "query": "query { user { email name } }" })).await?.body;
        if let Some(err) = s(&reply, "/errors/0/message") {
            return Err(ImportError::invalid(format!("Fireflies said: {err}")));
        }
        Ok(s(&reply, "/data/user/email").map(|e| format!("Connected as {e}")))
    }
}

fn fireflies_note(t: &Value) -> ImportedNote {
    let started = date(t, "/date");
    let duration = num(t, "/duration").map(|m| (m * 60_000.0).round() as u32);
    let turns: Vec<Turn> = arr(t, "/sentences")
        .iter()
        .filter_map(|x| {
            Some(Turn {
                speaker: speaker_ref(&s(x, "/speaker_name").unwrap_or_default()),
                start_ms: num(x, "/start_time").and_then(secs_to_ms),
                end_ms: num(x, "/end_time").and_then(secs_to_ms),
                text: s(x, "/text")?,
            })
        })
        .collect();
    let summary = [s(t, "/summary/overview").map(|o| format!("## Overview\n\n{o}")), s(t, "/summary/shorthand_bullet").or_else(|| s(t, "/summary/notes")).map(|n| format!("## Notes\n\n{n}"))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
    // Action items come as one string, grouped under bold owner names: "**Ana**\n- Send notes".
    let actions = s(t, "/summary/action_items")
        .map(|a| {
            let mut owner: Option<String> = None;
            let mut items = Vec::new();
            for line in a.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if line.starts_with("**") && line.trim_end_matches(':').ends_with("**") {
                    owner = Some(line.trim_matches(|c| c == '*' || c == ':').trim().to_string()).filter(|o| !o.is_empty());
                    continue;
                }
                let item = line.trim_start_matches(['-', '*', '•']).trim();
                if !item.is_empty() {
                    items.push(match &owner {
                        Some(o) => format!("{item} ({o})"),
                        None => item.to_string(),
                    });
                }
            }
            items
        })
        .unwrap_or_default();
    ImportedNote {
        external_id: s(t, "/id").unwrap_or_default(),
        title: s(t, "/title").unwrap_or_else(|| "Fireflies meeting".into()),
        created_at: started,
        started_at: started,
        ended_at: started.zip(duration).map(|(s, d)| s + TimeDelta::milliseconds(d as i64)),
        duration_ms: duration,
        meeting_app: meeting_app(s(t, "/meeting_link")),
        attendees: names(arr(t, "/meeting_attendees"), &["/displayName", "/name", "/email"]),
        summary_md: (!summary.is_empty()).then_some(summary),
        action_items: actions,
        transcript: (!turns.is_empty()).then(|| finish(turns)),
        meeting: true,
        ..Default::default()
    }
}

// ── Fathom ───────────────────────────────────────────────────────────────

pub struct Fathom;
const FATHOM: &str = "https://api.fathom.ai/external/v1";

fn fathom_http(key: &str, notice: Option<WaitNotice>) -> Http {
    // Transcript and summary calls are limited to 30 a minute.
    Http::new("Fathom", Duration::from_millis(2_100)).header("x-api-key", key.trim()).with_notice(notice)
}

#[async_trait]
impl Provider for Fathom {
    fn id(&self) -> &'static str {
        "fathom"
    }
    fn name(&self) -> &'static str {
        "Fathom"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Fathom.app"]
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Fathom")?;
        let http = fathom_http(&key, cx.notice.clone());
        let mut notes = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let url = format!(
                "{FATHOM}/meetings?include_transcript=true&include_summary=true&include_action_items=true{}",
                cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default()
            );
            let page = http.get(&url).await?.body;
            notes.extend(arr(&page, "/items").iter().map(fathom_note));
            cursor = s(&page, "/next_cursor");
            if cursor.is_none() || arr(&page, "/items").is_empty() {
                break;
            }
        }
        Ok(Scan { origin: "your Fathom account".into(), notes, ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let page = fathom_http(key, None).get(&format!("{FATHOM}/meetings")).await?.body;
        Ok(arr(&page, "/items").first().and_then(|m| s(m, "/recorded_by/email")).map(|e| format!("Connected as {e}")))
    }
}

fn fathom_note(m: &Value) -> ImportedNote {
    let me = s(m, "/recorded_by/name");
    let turns: Vec<Turn> = arr(m, "/transcript")
        .iter()
        .filter_map(|t| {
            let name = s(t, "/speaker/display_name").unwrap_or_default();
            let speaker = if me.as_deref().is_some_and(|me| me.eq_ignore_ascii_case(&name)) { SpeakerRef::Me } else { speaker_ref(&name) };
            Some(Turn { speaker, start_ms: s(t, "/timestamp").and_then(|c| parse_clock(&c)), end_ms: None, text: s(t, "/text")? })
        })
        .collect();
    let started = date(m, "/recording_start_time").or_else(|| date(m, "/scheduled_start_time"));
    let ended = date(m, "/recording_end_time").or_else(|| date(m, "/scheduled_end_time"));
    ImportedNote {
        external_id: s(m, "/recording_id").or_else(|| s(m, "/url")).unwrap_or_default(),
        title: s(m, "/title").or_else(|| s(m, "/meeting_title")).unwrap_or_else(|| "Fathom meeting".into()),
        created_at: date(m, "/created_at"),
        started_at: started,
        ended_at: ended,
        duration_ms: duration_between(started, ended),
        meeting_app: meeting_app(s(m, "/meeting_url")),
        attendees: names(arr(m, "/calendar_invitees"), &["/name", "/email"]),
        summary_md: s(m, "/default_summary/markdown_formatted"),
        action_items: arr(m, "/action_items")
            .iter()
            .filter_map(|a| s(a, "/description").map(|d| match s(a, "/assignee/name") {
                Some(who) => format!("{d} ({who})"),
                None => d,
            }))
            .collect(),
        transcript: (!turns.is_empty()).then(|| finish(turns)),
        meeting: true,
        ..Default::default()
    }
}

// ── Otter (Enterprise API) ───────────────────────────────────────────────

pub struct Otter;
const OTTER: &str = "https://api.otter.ai/v1";

fn otter_http(key: &str, notice: Option<WaitNotice>) -> Http {
    Http::new("Otter", Duration::from_millis(150)).bearer(key).with_notice(notice)
}

#[async_trait]
impl Provider for Otter {
    fn id(&self) -> &'static str {
        "otter"
    }
    fn name(&self) -> &'static str {
        "Otter"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Otter.app", "Otter.ai.app"]
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Otter")?;
        let http = otter_http(&key, cx.notice.clone());
        let mut remote = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let url = format!("{OTTER}/conversations?limit=20{}", cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default());
            let page = http.get(&url).await?.body;
            for c in arr(&page, "/data") {
                let Some(id) = s(c, "/id") else { continue };
                remote.push(RemoteEntry { external_id: id, title: s(c, "/title").unwrap_or_else(|| "Otter conversation".into()), date: date(c, "/created_at") });
            }
            cursor = s(&page, "/meta/next_cursor");
            if page.pointer("/meta/has_more").and_then(Value::as_bool) != Some(true) || cursor.is_none() {
                break;
            }
        }
        Ok(Scan { origin: "your Otter workspace".into(), remote, loader: Some(Arc::new(OtterLoader { key })), declared: Some(MEETINGS), ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let page = otter_http(key, None).get(&format!("{OTTER}/conversations?limit=1")).await?.body;
        Ok(arr(&page, "/data").first().and_then(|c| s(c, "/owner/email")).map(|e| format!("Connected as {e}")))
    }
}

struct OtterLoader {
    key: String,
}

#[async_trait]
impl Loader for OtterLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let body = otter_http(&self.key, notice).get(&format!("{OTTER}/conversations/{}?include=all", encode(&entry.external_id))).await?.body;
        let c = body.get("data").unwrap_or(&body);
        let transcript = s(c, "/relationships/transcript/content").and_then(|t| parse_text(&t));
        let outline: Vec<String> = arr(c, "/relationships/outline")
            .iter()
            .filter_map(|o| {
                let section = s(o, "/section")?;
                let lines: Vec<String> = arr(o, "/text").iter().filter_map(Value::as_str).map(|l| format!("- {l}")).collect();
                Some(format!("### {section}\n\n{}", lines.join("\n")))
            })
            .collect();
        let summary = [s(c, "/abstract_summary"), (!outline.is_empty()).then(|| format!("## Outline\n\n{}", outline.join("\n\n")))].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
        let started = date(c, "/created_at");
        Ok(ImportedNote {
            external_id: entry.external_id.clone(),
            title: s(c, "/title").unwrap_or_else(|| entry.title.clone()),
            created_at: started,
            started_at: started,
            duration_ms: transcript.as_ref().and_then(|t| t.turns.iter().filter_map(|x| x.end_ms).max()),
            meeting_app: meeting_app(s(c, "/conf_join_url")),
            attendees: names(arr(c, "/calendar_guests"), &["/name", "/email"]),
            summary_md: (!summary.is_empty()).then_some(summary),
            action_items: arr(c, "/relationships/action_items").iter().filter_map(|a| s(a, "/text")).collect(),
            transcript,
            meeting: true,
            ..Default::default()
        })
    }
}

// ── tl;dv ────────────────────────────────────────────────────────────────

pub struct Tldv;
const TLDV: &str = "https://pasta.tldv.io/v1alpha1";

fn tldv_http(key: &str, notice: Option<WaitNotice>) -> Http {
    Http::new("tl;dv", Duration::from_millis(250)).header("x-api-key", key.trim()).with_notice(notice)
}

#[async_trait]
impl Provider for Tldv {
    fn id(&self) -> &'static str {
        "tldv"
    }
    fn name(&self) -> &'static str {
        "tl;dv"
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "tl;dv")?;
        let http = tldv_http(&key, cx.notice.clone());
        let mut remote = Vec::new();
        let mut page_no = 1;
        loop {
            let page = http.get(&format!("{TLDV}/meetings?page={page_no}&limit=100")).await?.body;
            for m in arr(&page, "/results") {
                let Some(id) = s(m, "/id") else { continue };
                remote.push(RemoteEntry { external_id: id, title: s(m, "/name").unwrap_or_else(|| "tl;dv meeting".into()), date: date(m, "/happenedAt") });
            }
            let pages = num(&page, "/pages").unwrap_or(1.0) as usize;
            if page_no >= pages || arr(&page, "/results").is_empty() || page_no >= MAX_PAGES {
                break;
            }
            page_no += 1;
        }
        Ok(Scan { origin: "your tl;dv account".into(), remote, loader: Some(Arc::new(TldvLoader { key })), declared: Some(MEETINGS), ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let page = tldv_http(key, None).get(&format!("{TLDV}/meetings?page=1&limit=1")).await?.body;
        Ok(num(&page, "/total").map(|t| format!("{} meetings found", t as u64)))
    }
}

struct TldvLoader {
    key: String,
}

#[async_trait]
impl Loader for TldvLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let http = tldv_http(&self.key, notice);
        let id = encode(&entry.external_id);
        let meeting = http.get(&format!("{TLDV}/meetings/{id}")).await?.body;
        let transcript = http.get(&format!("{TLDV}/meetings/{id}/transcript")).await.map(|r| r.body).unwrap_or(Value::Null);
        let notes = http.get(&format!("{TLDV}/meetings/{id}/notes")).await.map(|r| r.body).unwrap_or(Value::Null);
        let turns: Vec<Turn> = arr(&transcript, "/data")
            .iter()
            .filter_map(|t| Some(Turn { speaker: speaker_ref(&s(t, "/speaker").unwrap_or_default()), start_ms: num(t, "/startTime").and_then(secs_to_ms), end_ms: num(t, "/endTime").and_then(secs_to_ms), text: s(t, "/text")? }))
            .collect();
        let started = date(&meeting, "/happenedAt").or(entry.date);
        let duration = num(&meeting, "/duration").and_then(secs_to_ms);
        Ok(ImportedNote {
            external_id: entry.external_id.clone(),
            title: s(&meeting, "/name").unwrap_or_else(|| entry.title.clone()),
            created_at: started,
            started_at: started,
            ended_at: started.zip(duration).map(|(s, d)| s + TimeDelta::milliseconds(d as i64)),
            duration_ms: duration,
            attendees: names(arr(&meeting, "/invitees"), &["/name", "/email"]),
            summary_md: s(&notes, "/markdownContent"),
            transcript: (!turns.is_empty()).then(|| finish(turns)),
            meeting: true,
            ..Default::default()
        })
    }
}

// ── Circleback ───────────────────────────────────────────────────────────

pub struct Circleback;
const CIRCLEBACK: &str = "https://circleback.ai/api";

fn circleback_http(key: &str, notice: Option<WaitNotice>) -> Http {
    // Free plans allow 20 requests a minute.
    Http::new("Circleback", Duration::from_millis(3_100)).bearer(key).with_notice(notice)
}

#[async_trait]
impl Provider for Circleback {
    fn id(&self) -> &'static str {
        "circleback"
    }
    fn name(&self) -> &'static str {
        "Circleback"
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Circleback")?;
        let http = circleback_http(&key, cx.notice.clone());
        let mut notes = Vec::new();
        let mut url = format!("{CIRCLEBACK}/meetings");
        for _ in 0..MAX_PAGES {
            let reply = http.get(&url).await?;
            let list = reply.body.as_array().cloned().unwrap_or_default();
            notes.extend(list.iter().map(circleback_note));
            match reply.next_link {
                Some(next) if !list.is_empty() => url = if next.starts_with("http") { next } else { format!("https://circleback.ai{next}") },
                _ => break,
            }
        }
        // Transcripts come one meeting at a time, while importing.
        let remote = notes.iter().map(|n| RemoteEntry { external_id: n.external_id.clone(), title: n.title.clone(), date: n.date() }).collect();
        Ok(Scan { origin: "your Circleback account".into(), remote, loader: Some(Arc::new(CirclebackLoader { key, notes })), declared: Some(ImportFidelity { my_notes: true, ..MEETINGS }), ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let reply = circleback_http(key, None).get(&format!("{CIRCLEBACK}/meetings")).await?;
        Ok(reply.body.as_array().map(|a| if a.is_empty() { "No meetings yet".to_string() } else { "Connected".to_string() }))
    }
}

fn circleback_note(m: &Value) -> ImportedNote {
    let created = date(m, "/createdAt");
    let duration = num(m, "/duration").and_then(secs_to_ms);
    ImportedNote {
        external_id: s(m, "/id").unwrap_or_default(),
        title: s(m, "/name").unwrap_or_else(|| "Circleback meeting".into()),
        created_at: created,
        updated_at: date(m, "/updatedAt"),
        started_at: created,
        ended_at: created.zip(duration).map(|(s, d)| s + TimeDelta::milliseconds(d as i64)),
        duration_ms: duration,
        meeting_app: meeting_app(s(m, "/url")),
        attendees: names(arr(m, "/attendees"), &["/name", "/email"]),
        summary_md: s(m, "/notes"),
        notes_md: s(m, "/privateNotes"),
        action_items: arr(m, "/actionItems")
            .iter()
            .filter_map(|a| s(a, "/title").map(|t| match s(a, "/assignee/name") {
                Some(who) => format!("{t} ({who})"),
                None => t,
            }))
            .collect(),
        tags: arr(m, "/tags").iter().filter_map(|t| s(t, "/name")).collect(),
        meeting: true,
        ..Default::default()
    }
}

struct CirclebackLoader {
    key: String,
    notes: Vec<ImportedNote>,
}

#[async_trait]
impl Loader for CirclebackLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let mut note = self.notes.iter().find(|n| n.external_id == entry.external_id).cloned().ok_or_else(|| ImportError::not_found("That Circleback meeting is gone."))?;
        let reply = circleback_http(&self.key, notice).get(&format!("{CIRCLEBACK}/meeting/{}/transcript", encode(&entry.external_id))).await?;
        let turns: Vec<Turn> = reply
            .body
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| Some(Turn { speaker: speaker_ref(&s(t, "/speaker").unwrap_or_default()), start_ms: num(t, "/timestamp").and_then(secs_to_ms), end_ms: None, text: s(t, "/text")? }))
            .collect();
        note.transcript = (!turns.is_empty()).then(|| finish(turns));
        Ok(note)
    }
}

// ── MeetGeek ─────────────────────────────────────────────────────────────

pub struct MeetGeek;
const MEETGEEK_REGIONS: [&str; 2] = ["https://api.meetgeek.ai", "https://api-us.meetgeek.ai"];

fn meetgeek_http(key: &str, notice: Option<WaitNotice>) -> Http {
    Http::new("MeetGeek", Duration::from_millis(700)).bearer(key).with_notice(notice)
}

/// MeetGeek keys only work in their own region (EU or US): find which.
async fn meetgeek_base(key: &str) -> Result<&'static str, ImportError> {
    let mut last = None;
    for base in MEETGEEK_REGIONS {
        match meetgeek_http(key, None).get(&format!("{base}/v1/meetings?limit=1")).await {
            Ok(_) => return Ok(base),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| ImportError::invalid("MeetGeek didn't accept the API key.")))
}

#[async_trait]
impl Provider for MeetGeek {
    fn id(&self) -> &'static str {
        "meetgeek"
    }
    fn name(&self) -> &'static str {
        "MeetGeek"
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "MeetGeek")?;
        let base = meetgeek_base(&key).await?;
        let http = meetgeek_http(&key, cx.notice.clone());
        let mut remote = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let page = http.get(&format!("{base}/v1/meetings?limit=500{}", cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default())).await?.body;
            for m in arr(&page, "/meetings") {
                let Some(id) = s(m, "/meeting_id") else { continue };
                remote.push(RemoteEntry { external_id: id, title: "MeetGeek meeting".into(), date: date(m, "/timestamp_start_utc") });
            }
            cursor = s(&page, "/pagination/next_cursor");
            if cursor.is_none() || arr(&page, "/meetings").is_empty() {
                break;
            }
        }
        Ok(Scan {
            origin: "your MeetGeek account".into(),
            remote,
            loader: Some(Arc::new(MeetGeekLoader { key, base })),
            declared: Some(MEETINGS),
            warnings: vec!["MeetGeek's Free and Pro plans allow 100 API calls a day and each meeting takes three, so a large account may need several days. Run the import again each day; it picks up where it stopped.".into()],
            ..Default::default()
        })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        let base = meetgeek_base(key).await?;
        Ok(Some(if base.contains("-us") { "Connected (US region)".into() } else { "Connected (EU region)".into() }))
    }
}

struct MeetGeekLoader {
    key: String,
    base: &'static str,
}

#[async_trait]
impl Loader for MeetGeekLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let http = meetgeek_http(&self.key, notice);
        let id = encode(&entry.external_id);
        let base = self.base;
        let meeting = http.get(&format!("{base}/v1/meetings/{id}")).await?.body;
        let summary = http.get(&format!("{base}/v1/meetings/{id}/summary")).await.map(|r| r.body).unwrap_or(Value::Null);
        let started = date(&meeting, "/timestamp_start_utc").or(entry.date);
        let ended = date(&meeting, "/timestamp_end_utc");
        let mut turns = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..50 {
            let Ok(page) = http.get(&format!("{base}/v1/meetings/{id}/transcript?limit=500{}", cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default())).await.map(|r| r.body) else { break };
            for x in arr(&page, "/sentences") {
                let Some(text) = s(x, "/transcript").or_else(|| s(x, "/text")) else { continue };
                // The timestamp's format is not documented: a clock, seconds, or a date.
                let at = s(x, "/timestamp").and_then(|t| parse_clock(&t).or_else(|| t.parse::<f64>().ok().and_then(secs_to_ms)).or_else(|| started.zip(parse_date(&t)).map(|(z, d)| (d - z).num_milliseconds().clamp(0, u32::MAX as i64) as u32)));
                turns.push(Turn { speaker: speaker_ref(&s(x, "/speaker").unwrap_or_default()), start_ms: at, end_ms: None, text });
            }
            cursor = s(&page, "/pagination/next_cursor");
            if cursor.is_none() {
                break;
            }
        }
        let body = [s(&summary, "/summary"), s(&summary, "/ai_insights").map(|i| format!("## Insights\n\n{i}"))].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
        Ok(ImportedNote {
            external_id: entry.external_id.clone(),
            title: s(&meeting, "/title").unwrap_or_else(|| entry.title.clone()),
            created_at: started,
            started_at: started,
            ended_at: ended,
            duration_ms: duration_between(started, ended),
            meeting_app: meeting_app(s(&meeting, "/join_link")),
            attendees: arr(&meeting, "/participant_emails").iter().filter_map(Value::as_str).map(str::to_string).collect(),
            summary_md: (!body.is_empty()).then_some(body),
            transcript: (!turns.is_empty()).then(|| finish(turns)),
            meeting: true,
            ..Default::default()
        })
    }
}

// ── Jamie ────────────────────────────────────────────────────────────────

pub struct Jamie;
const JAMIE: &str = "https://beta-api.meetjamie.ai/v1/me";

fn jamie_http(key: &str, notice: Option<WaitNotice>) -> Http {
    Http::new("Jamie", Duration::from_millis(650)).header("x-api-key", key.trim()).with_notice(notice)
}

fn jamie_url(method: &str, input: Value) -> String {
    format!("{JAMIE}/{method}?input={}", encode(&json!({ "json": input }).to_string()))
}

#[async_trait]
impl Provider for Jamie {
    fn id(&self) -> &'static str {
        "jamie"
    }
    fn name(&self) -> &'static str {
        "Jamie"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Jamie.app"]
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Jamie")?;
        let http = jamie_http(&key, cx.notice.clone());
        let mut remote = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut input = json!({ "limit": 100 });
            if let Some(c) = &cursor {
                input["cursor"] = json!(c);
            }
            let page = http.get(&jamie_url("meetings.list", input)).await?.body;
            let data = page.pointer("/result/data/json").unwrap_or(&page);
            for m in arr(data, "/meetings") {
                let Some(id) = s(m, "/id") else { continue };
                remote.push(RemoteEntry { external_id: id, title: s(m, "/title").or_else(|| s(m, "/generatedTitle")).unwrap_or_else(|| "Jamie meeting".into()), date: date(m, "/startTime") });
            }
            cursor = s(data, "/nextCursor");
            if cursor.is_none() || arr(data, "/meetings").is_empty() {
                break;
            }
        }
        Ok(Scan {
            origin: "your Jamie account".into(),
            remote,
            loader: Some(Arc::new(JamieLoader { key })),
            declared: Some(ImportFidelity { my_notes: true, timestamps: false, ..MEETINGS }),
            ..Default::default()
        })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        jamie_http(key, None).get(&jamie_url("meetings.list", json!({ "limit": 1 }))).await?;
        Ok(Some("Connected".into()))
    }
}

struct JamieLoader {
    key: String,
}

#[async_trait]
impl Loader for JamieLoader {
    async fn load(&self, entry: &RemoteEntry, notice: Option<WaitNotice>) -> Result<ImportedNote, ImportError> {
        let body = jamie_http(&self.key, notice).get(&jamie_url("meetings.get", json!({ "meetingId": entry.external_id }))).await?.body;
        let m = body.pointer("/result/data/json").unwrap_or(&body);
        let started = date(m, "/startTime").or(entry.date);
        let ended = date(m, "/endTime");
        let transcript = s(m, "/transcript").and_then(|t| parse_text(&t));
        Ok(ImportedNote {
            external_id: entry.external_id.clone(),
            title: s(m, "/title").or_else(|| s(m, "/generatedTitle")).unwrap_or_else(|| entry.title.clone()),
            created_at: started,
            started_at: started,
            ended_at: ended,
            duration_ms: duration_between(started, ended),
            attendees: names(arr(m, "/participants"), &["/name", "/email"]),
            summary_md: s(m, "/summary/markdown").or_else(|| s(m, "/summary/short")),
            notes_md: s(m, "/scratchpadNotes"),
            action_items: arr(m, "/tasks").iter().filter_map(|t| s(t, "/content")).collect(),
            tags: arr(m, "/tags").iter().filter_map(|t| s(t, "/name")).collect(),
            transcript,
            meeting: true,
            ..Default::default()
        })
    }
}

// ── Limitless ────────────────────────────────────────────────────────────

pub struct Limitless;
const LIMITLESS: &str = "https://api.limitless.ai/v1";

fn limitless_http(key: &str, notice: Option<WaitNotice>) -> Http {
    // 180 requests a minute.
    Http::new("Limitless", Duration::from_millis(350)).header("x-api-key", key.trim()).with_notice(notice)
}

#[async_trait]
impl Provider for Limitless {
    fn id(&self) -> &'static str {
        "limitless"
    }
    fn name(&self) -> &'static str {
        "Limitless"
    }
    fn apps(&self) -> &'static [&'static str] {
        &["Limitless.app", "Rewind.app"]
    }

    async fn scan(&self, input: &ImportInput, cx: &Cx<'_>) -> Result<Scan, ImportError> {
        if let Some(r) = files_or_paste(input).await {
            return r;
        }
        let key = need_key(cx, "Limitless")?;
        let http = limitless_http(&key, cx.notice.clone());
        let mut notes = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20_000 {
            let url = format!(
                "{LIMITLESS}/lifelogs?includeContents=true&includeMarkdown=false&limit=10&direction=asc&timezone=UTC{}",
                cursor.as_deref().map(|c| format!("&cursor={}", encode(c))).unwrap_or_default()
            );
            let page = http.get(&url).await?.body;
            let logs = arr(&page, "/data/lifelogs");
            notes.extend(logs.iter().map(limitless_note));
            cursor = s(&page, "/meta/lifelogs/nextCursor");
            if cursor.is_none() || logs.is_empty() {
                break;
            }
        }
        Ok(Scan { origin: "your Limitless account".into(), notes, ..Default::default() })
    }

    async fn check_key(&self, key: &str) -> Result<Option<String>, ImportError> {
        limitless_http(key, None).get(&format!("{LIMITLESS}/lifelogs?limit=1&includeContents=false")).await?;
        Ok(Some("Connected".into()))
    }
}

fn limitless_note(l: &Value) -> ImportedNote {
    let mut outline = Vec::new();
    let mut turns = Vec::new();
    fn walk(items: &[Value], outline: &mut Vec<String>, turns: &mut Vec<Turn>) {
        for c in items {
            let kind = s(c, "/type").unwrap_or_default();
            let content = s(c, "/content").unwrap_or_default();
            match kind.as_str() {
                "heading1" => {}
                "heading2" => outline.push(format!("## {content}")),
                "heading3" => outline.push(format!("### {content}")),
                "blockquote" => {
                    let speaker = if s(c, "/speakerIdentifier").as_deref() == Some("user") { SpeakerRef::Me } else { speaker_ref(&s(c, "/speakerName").unwrap_or_default()) };
                    turns.push(Turn { speaker, start_ms: num(c, "/startOffsetMs").map(|n| n as u32), end_ms: num(c, "/endOffsetMs").map(|n| n as u32), text: content });
                }
                _ if !content.is_empty() => outline.push(content),
                _ => {}
            }
            walk(arr(c, "/children"), outline, turns);
        }
    }
    walk(arr(l, "/contents"), &mut outline, &mut turns);
    let started = date(l, "/startTime");
    let ended = date(l, "/endTime");
    ImportedNote {
        external_id: s(l, "/id").unwrap_or_default(),
        title: s(l, "/title").unwrap_or_else(|| "Limitless recording".into()),
        created_at: started,
        updated_at: date(l, "/updatedAt"),
        started_at: started,
        ended_at: ended,
        duration_ms: duration_between(started, ended),
        summary_md: (!outline.is_empty()).then(|| outline.join("\n\n")),
        attendees: ImportedTranscript::new(turns.clone()).names(),
        transcript: (!turns.is_empty()).then(|| finish(turns)),
        meeting: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fireflies_transcripts() {
        let t: Value = serde_json::from_str(
            r#"{"id":"abc","title":"Standup","date":1706715000000,"duration":"1.5","meeting_link":"https://meet.google.com/x",
            "meeting_attendees":[{"displayName":"Ana","email":"a@x"},{"name":null,"email":"b@x"}],
            "sentences":[{"speaker_name":"Ana","text":"Hi","start_time":"0.5","end_time":"1.2"},{"speaker_name":"Bo","text":"Yo","start_time":2,"end_time":3}],
            "summary":{"overview":"Short","action_items":"**Ana**\n- Send notes\n- Book room"}}"#,
        )
        .unwrap();
        let n = fireflies_note(&t);
        assert_eq!(n.meeting_app.as_deref(), Some("Google Meet"));
        assert_eq!(n.duration_ms, Some(90_000));
        assert_eq!(n.attendees, vec!["Ana", "b@x"]);
        assert_eq!(n.action_items, vec!["Send notes (Ana)", "Book room (Ana)"]);
        let tr = n.transcript.unwrap();
        assert_eq!(tr.turns[0].start_ms, Some(500));
        assert_eq!(tr.turns[1].start_ms, Some(2_000));
        assert!(n.summary_md.unwrap().contains("## Overview"));
    }

    #[test]
    fn fathom_meetings_know_who_recorded() {
        let m: Value = serde_json::from_str(
            r#####"{"title":"Sync","recording_id":42,"recording_start_time":"2025-03-04T09:00:00Z","recording_end_time":"2025-03-04T09:30:00Z",
            "recorded_by":{"name":"Sam Lee"},"meeting_url":"https://zoom.us/j/1",
            "transcript":[{"speaker":{"display_name":"Sam Lee"},"text":"Hello","timestamp":"00:00:05"},{"speaker":{"display_name":"Ana"},"text":"Hi","timestamp":"00:01:00"}],
            "default_summary":{"markdown_formatted":"## Summary\nGood"},"action_items":[{"description":"Send deck","assignee":{"name":"Ana"}}]}"#####,
        )
        .unwrap();
        let n = fathom_note(&m);
        assert_eq!(n.external_id, "42");
        assert_eq!(n.duration_ms, Some(1_800_000));
        assert_eq!(n.meeting_app.as_deref(), Some("Zoom"));
        let tr = n.transcript.unwrap();
        assert_eq!(tr.turns[0].speaker, SpeakerRef::Me);
        assert_eq!(tr.turns[0].end_ms, Some(60_000));
        assert_eq!(n.action_items, vec!["Send deck (Ana)"]);
    }

    #[test]
    fn limitless_lifelogs() {
        let l: Value = serde_json::from_str(
            r#"{"id":"L1","title":"Coffee chat","startTime":"2025-03-04T09:00:00Z","endTime":"2025-03-04T09:10:00Z",
            "contents":[{"type":"heading1","content":"Coffee chat"},{"type":"heading2","content":"Plans","children":[
              {"type":"blockquote","content":"Morning","speakerIdentifier":"user","startOffsetMs":0,"endOffsetMs":900},
              {"type":"blockquote","content":"Hey","speakerName":"Ana","startOffsetMs":1000,"endOffsetMs":1500}]}]}"#,
        )
        .unwrap();
        let n = limitless_note(&l);
        assert_eq!(n.summary_md.as_deref(), Some("## Plans"));
        let tr = n.transcript.unwrap();
        assert_eq!(tr.turns.len(), 2);
        assert_eq!(tr.turns[0].speaker, SpeakerRef::Me);
        assert_eq!(n.attendees, vec!["Ana"]);
    }

    #[test]
    fn circleback_meetings() {
        let m: Value = serde_json::from_str(
            r#####"{"id":"W1","name":"Review","createdAt":"2025-03-04T09:00:00Z","duration":60.5,"notes":"#### Overview\nok","privateNotes":"mine",
            "attendees":[{"name":"Ana"}],"actionItems":[{"title":"Ship","assignee":null}],"tags":[{"name":"design"}]}"#####,
        )
        .unwrap();
        let n = circleback_note(&m);
        assert_eq!(n.duration_ms, Some(60_500));
        assert_eq!(n.notes_md.as_deref(), Some("mine"));
        assert_eq!(n.action_items, vec!["Ship"]);
        assert_eq!(n.tags, vec!["design"]);
    }
}

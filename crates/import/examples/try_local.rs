//! Try the importer against what is really on this computer, writing into a throwaway database.
//! Nothing is changed in the other apps, and Margin's own database is not touched.
//!
//! ```sh
//! cargo run -p margin-import --example try_local -- obsidian
//! cargo run -p margin-import --example try_local -- anarlog --counts-only
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use margin_domain::{AppEvent, ImportCategoryMode, ImportInput, ImportOptions, ImportPhase, RecordingSink};
use margin_import::{Importer, KnownPaths, Secrets};
use margin_store::Db;

struct NoSecrets;
impl Secrets for NoSecrets {
    fn key(&self, _: &str) -> Option<String> {
        None
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let provider = args.next().unwrap_or_else(|| "obsidian".into());
    let counts_only = args.any(|a| a == "--counts-only");
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let support = home.join("Library/Application Support");
    let paths = KnownPaths {
        home: home.clone(),
        config_dir: support.clone(),
        data_dir: support.clone(),
        local_data_dir: support,
        applications: vec![PathBuf::from("/Applications"), home.join("Applications"), PathBuf::from("/System/Applications")],
    };
    let db = Db::open_in_memory().await.expect("db");
    let sink = Arc::new(RecordingSink(Default::default()));
    let importer = Importer::new(db.clone(), sink.clone(), paths, Arc::new(NoSecrets));

    let detection = importer.detect(&provider).await.expect("detect");
    println!("installed: {}  locations: {}", detection.app_installed, detection.locations.len());
    for l in &detection.locations {
        println!("  {} · notes {} · meetings {} · transcripts {} · images {}", l.label, l.notes, l.meetings, l.transcripts, l.attachments);
        for u in &l.unsupported {
            println!("    left out: {} ({})", u.kind, u.count);
        }
    }
    if let Some(n) = &detection.note {
        println!("  note: {n}");
    }
    let Some(first) = detection.locations.first() else { return };

    let preview = importer.scan(&provider, ImportInput::Local { path: Some(first.path.clone()) }).await.expect("scan");
    println!(
        "preview: total {} · notes {} · meetings {} · transcripts {} · images {} · folders {:?} · speakers {} · includes {:?}",
        preview.total,
        preview.notes,
        preview.meetings,
        preview.transcripts,
        preview.attachments,
        preview.folders,
        preview.speakers.len(),
        preview.includes
    );
    for u in &preview.unsupported {
        println!("  left out: {} ({}): {}", u.kind, u.count, u.reason);
    }
    for w in &preview.warnings {
        println!("  warning: {w}");
    }

    importer.start(preview.scan_id, ImportOptions { category_mode: ImportCategoryMode::KeepFolders, self_names: vec![] }).await.expect("start");
    let run = loop {
        let done = sink.0.lock().unwrap().iter().find_map(|e| match e {
            AppEvent::ImportProgress { progress } if progress.phase == ImportPhase::Done => progress.run.clone(),
            _ => None,
        });
        if let Some(run) = done {
            break run;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    println!("run: {:?} · imported {} · skipped {} · failed {}", run.status, run.imported, run.skipped, run.failed);
    for line in &run.report {
        println!("  report: {line}");
    }
    let categories = db.list_categories().await.unwrap();
    println!("categories: {:?}", categories.iter().map(|c| c.name.as_str()).collect::<Vec<_>>());
    for n in db.list_notes().await.unwrap() {
        let note = db.get_note(n.id).await.unwrap();
        let t = db.get_transcript(n.id).await.unwrap();
        if counts_only {
            println!("  - {:?} · {} chars · {} segments · {} speakers · timed {}", n.kind, note.body_md.len(), t.segments.len(), t.speakers.len(), t.timed);
        } else {
            println!("  - “{}” ({:?}, created {}) · {} chars · {} segments", n.title, n.kind, n.created_at.format("%Y-%m-%d"), note.body_md.len(), t.segments.len());
            println!("    body: {:?}", note.body_md.chars().take(120).collect::<String>());
        }
    }
}

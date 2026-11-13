/**
 * Every app Cilica imports from: how people get their notes out of it, step by step, and what
 * comes across. The ids match `crates/import/src/providers/mod.rs`.
 *
 * Steps use a little markup: `**Settings › API**` is something to click (menus use ›), and
 * `` `takeout.google.com` `` is something to type or a file name.
 */
import { FolderOpen, type LucideIcon } from "lucide-react";

/** Each app's own icon, from its website, the App Store or macOS, as a 96 px square. */
const LOGOS = import.meta.glob<string>("./logos/*.{png,svg}", {
  eager: true,
  import: "default",
  query: "?url",
});
/** Apps that share another's icon. */
const SAME_LOGO: Record<string, string> = { notion_meetings: "notion" };

function logoFor(id: string): string | undefined {
  const name = SAME_LOGO[id] ?? id;
  return LOGOS[`./logos/${name}.png`] ?? LOGOS[`./logos/${name}.svg`];
}

export type ProviderGroup = "notes" | "meetings" | "platforms" | "other";

export const GROUP_LABELS: Record<ProviderGroup, string> = {
  notes: "Notes apps",
  meetings: "Meeting note takers",
  platforms: "Meeting apps",
  other: "Anything else",
};

/** How a person gets notes from the app into Cilica. */
export type ImportRoute =
  | {
      kind: "local";
      label: string;
      /** Shown when nothing was found yet, or before asking macOS (Apple Notes). */
      steps?: string[];
      /** Read without a detected location (Apple Notes asks macOS first). */
      askFirst?: boolean;
      button?: string;
    }
  | {
      kind: "files";
      label: string;
      steps: string[];
      /** Extensions offered in the file picker, without dots. */
      accept: string[];
      /** Offer a folder picker too (vaults, graphs, unzipped exports). */
      folder?: boolean;
    }
  | {
      kind: "key";
      label: string;
      steps: string[];
      /** Which plans include API access, in a sentence. */
      plan?: string;
      placeholder?: string;
    }
  | { kind: "paste"; label: string; steps: string[] };

export interface ImportProvider {
  id: string;
  name: string;
  group: ProviderGroup;
  /** The app's icon (an image URL), filled in from `logos/` by id. */
  logo?: string;
  /** For entries that are not an app ("Other files"): a symbol instead. */
  glyph?: LucideIcon;
  /** One line under the name. */
  tagline: string;
  keywords?: string[];
  /** What comes across, at best. */
  carries: string[];
  routes: ImportRoute[];
  /** What never comes across, and why. */
  leftOut?: { kind: string; reason: string }[];
}

// ── Shared pieces ─────────────────────────────────────────────────────────

const TRANSCRIPT_FILES = ["txt", "vtt", "srt", "docx", "md", "json", "csv", "zip"];
const DOC_FILES = ["md", "markdown", "txt", "docx", "html", "zip"];

const recordings = (name: string) => ({
  kind: "Recordings",
  reason: `Audio and video stay in ${name}. Cilica imports the words.`,
});

const pasteRoute = (where: string): ImportRoute => ({
  kind: "paste",
  label: "Paste",
  steps: [
    `Open the meeting or note in ${where} and copy its text, transcript included.`,
    "Paste it below. Lines like `Sam: Hello` or `Sam  0:03` become a transcript with speakers.",
  ],
});

// ── The catalogue ─────────────────────────────────────────────────────────

const CATALOGUE: readonly ImportProvider[] = [
  // Notes apps
  {
    id: "obsidian",
    name: "Obsidian",
    group: "notes",
    tagline: "Vaults of Markdown notes",
    keywords: ["vault", "markdown", "md"],
    carries: ["Notes", "Folders as categories", "Tags and properties", "Images", "Dates"],
    routes: [
      { kind: "local", label: "On this Mac" },
      {
        kind: "files",
        label: "Choose a vault",
        folder: true,
        accept: ["md", "zip"],
        steps: [
          "In Obsidian, click the vault name at the bottom left and choose **Manage vaults…** to see where the vault lives.",
          "Choose that folder below, or drop it here. Cilica reads it and changes nothing in it.",
        ],
      },
    ],
    leftOut: [
      {
        kind: "Canvases",
        reason: "Cilica has no canvas view. The notes on a canvas are imported on their own.",
      },
      {
        kind: "Bases",
        reason: "Bases are database views. The notes they list are imported on their own.",
      },
      { kind: "Excalidraw drawings", reason: "Drawings made with the Excalidraw plugin." },
      {
        kind: "Plugins and settings",
        reason: "Themes, plugins and the `.obsidian` folder belong to Obsidian.",
      },
      {
        kind: "Dataview queries",
        reason: "Kept as code blocks. Cilica does not run them.",
      },
      {
        kind: "Links and embeds",
        reason: "`[[Links]]` become plain text and an embedded note shows as its name.",
      },
      { kind: "PDFs and other files", reason: "Only images come along, inside their notes." },
    ],
  },
  {
    id: "apple_notes",
    name: "Apple Notes",
    group: "notes",
    tagline: "Notes on your Mac and iCloud",
    keywords: ["icloud", "notes app", "mac"],
    carries: ["Notes", "Folders as categories", "Images", "Dates"],
    routes: [
      {
        kind: "local",
        label: "Read from Notes",
        askFirst: true,
        button: "Read Apple Notes",
        steps: [
          "Cilica asks Notes for your notes. macOS will ask whether Cilica may control Notes: choose **OK**.",
          "If you said no before, turn Notes on in **System Settings › Privacy & Security › Automation › Cilica**.",
        ],
      },
      {
        kind: "files",
        label: "Markdown export",
        accept: ["md", "markdown", "txt", "html"],
        folder: true,
        steps: [
          "In Notes on macOS 26 or later, select the notes, then choose **File › Export As › Markdown**.",
          "Choose the exported files or folder below, or drop them here.",
        ],
      },
    ],
    leftOut: [
      { kind: "Locked notes", reason: "Unlock them in Notes first, then import again." },
      {
        kind: "Attachments",
        reason: "Scans, drawings, PDFs and audio stay in Notes. Text and images come across.",
      },
    ],
  },
  {
    id: "notion",
    name: "Notion",
    group: "notes",
    tagline: "Pages and databases",
    keywords: ["wiki", "workspace"],
    carries: ["Pages", "Page hierarchy as categories", "Images"],
    routes: [
      {
        kind: "files",
        label: "Export zip",
        accept: ["zip", "md", "html", "csv"],
        folder: true,
        steps: [
          "For everything: in Notion, open **Settings › General** and choose **Export all workspace content**. For one page, open its **⋯** menu and choose **Export**.",
          "Pick **Markdown & CSV** and turn on **Include subpages** and **Create folders for subpages**.",
          "Download the zip Notion sends you and drop it here. There's no need to unzip it.",
        ],
      },
    ],
    leftOut: [
      {
        kind: "Database views",
        reason: "Each database row comes across as its own note. Table and board views do not.",
      },
      { kind: "Comments and page history", reason: "Notion does not include them in exports." },
    ],
  },
  {
    id: "evernote",
    name: "Evernote",
    group: "notes",
    tagline: "Notebooks exported as .enex",
    keywords: ["enex", "notebook"],
    carries: ["Notes", "Notebooks as categories", "Tags", "Images", "Checklists", "Dates"],
    routes: [
      {
        kind: "files",
        label: "ENEX export",
        accept: ["enex", "zip"],
        steps: [
          "In the Evernote desktop app, right-click a notebook and choose **Export notebook…**.",
          "Pick **ENEX format (.enex)** and save. Do this for each notebook; the file name becomes the category.",
          "Choose the .enex files below, or drop them here.",
        ],
      },
    ],
    leftOut: [
      { kind: "Encrypted text", reason: "Text you encrypted in Evernote stays encrypted there." },
      { kind: "PDFs and other files", reason: "Only images come along, inside their notes." },
      { kind: "Reminders", reason: "Cilica has no reminders." },
    ],
  },
  {
    id: "bear",
    name: "Bear",
    group: "notes",
    tagline: "Notes and tags from Bear",
    keywords: ["textbundle", "bear2bk"],
    carries: ["Notes", "Tags", "Images", "Dates"],
    routes: [
      {
        kind: "files",
        label: "Backup",
        accept: ["bear2bk", "zip", "textbundle", "md"],
        folder: true,
        steps: [
          "In Bear on your Mac, choose **File › Backup notes…** and save the backup.",
          "Drop the `.bear2bk` file here, or choose it below. Markdown or TextBundle exports work too.",
        ],
      },
    ],
    leftOut: [
      { kind: "Notes in Trash", reason: "Bear's Trash stays behind." },
      {
        kind: "Tag structure",
        reason: "Tags stay in the text as `#tags`; there's no tag sidebar.",
      },
    ],
  },
  {
    id: "logseq",
    name: "Logseq",
    group: "notes",
    tagline: "Pages and journals",
    keywords: ["graph", "journal", "outliner"],
    carries: ["Pages", "Journals with their dates", "Properties", "Images"],
    routes: [
      {
        kind: "files",
        label: "Graph folder",
        folder: true,
        accept: ["md", "zip"],
        steps: [
          "Choose your graph folder, the one holding `journals` and `pages`.",
          "On a database graph, first choose **⋯ › Export graph › Export as standard Markdown**, then choose that export.",
        ],
      },
    ],
    leftOut: [
      {
        kind: "Block references",
        reason: "They become “(block reference)”. The block stays in its own page.",
      },
      {
        kind: "Whiteboards and queries",
        reason: "Queries are kept as text; whiteboards are left out.",
      },
    ],
  },
  {
    id: "google_keep",
    name: "Google Keep",
    group: "notes",
    tagline: "Notes and lists from Takeout",
    keywords: ["takeout", "google"],
    carries: ["Notes", "Checklists", "Labels", "Images", "Dates"],
    routes: [
      {
        kind: "files",
        label: "Google Takeout",
        accept: ["zip", "json"],
        folder: true,
        steps: [
          "Go to `takeout.google.com`, choose **Deselect all**, then tick only **Keep**.",
          "Create the export and download the zip from the email Google sends.",
          "Drop the zip here. Notes in Keep's Trash are left out.",
        ],
      },
    ],
    leftOut: [{ kind: "Reminders and colours", reason: "Cilica has neither." }],
  },
  {
    id: "google_docs",
    name: "Google Docs",
    group: "notes",
    tagline: "Documents as Markdown or Word",
    keywords: ["drive", "docs", "gemini"],
    carries: ["Documents", "Headings and lists", "Transcripts inside them"],
    routes: [
      {
        kind: "files",
        label: "Downloaded files",
        accept: DOC_FILES,
        folder: true,
        steps: [
          "Open the document and choose **File › Download › Markdown (.md)** or **Microsoft Word (.docx)**.",
          "For many documents, use `takeout.google.com` with only **Drive** ticked.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "onenote",
    name: "OneNote",
    group: "notes",
    tagline: "Pages as Word documents",
    keywords: ["microsoft", "notebook"],
    carries: ["Pages", "Headings and lists"],
    routes: [
      {
        kind: "files",
        label: "Word export",
        accept: ["docx", "html", "txt"],
        steps: [
          "In OneNote for Windows, open a page or section and choose **File › Export › Word Document**.",
          "Choose the .docx files below, or drop them here.",
        ],
      },
      pasteRoute("OneNote"),
    ],
    leftOut: [
      { kind: "Ink and layout", reason: "Handwriting and free placement of text are lost." },
    ],
  },
  {
    id: "craft",
    name: "Craft",
    group: "notes",
    tagline: "Documents and spaces",
    carries: ["Documents", "Images"],
    routes: [
      {
        kind: "files",
        label: "Export",
        accept: ["md", "textbundle", "zip"],
        folder: true,
        steps: [
          "In Craft on your Mac, export a document or space as **Markdown** or **TextBundle**.",
          "Choose the export below, or drop it here.",
        ],
      },
    ],
  },
  {
    id: "reflect",
    name: "Reflect",
    group: "notes",
    tagline: "Notes and daily notes",
    carries: ["Notes", "Daily notes"],
    routes: [
      {
        kind: "files",
        label: "Markdown zip",
        accept: ["zip", "md"],
        folder: true,
        steps: [
          "In Reflect, open **Preferences**, choose your graph and **Export** it as **Markdown**.",
          "Drop the zip here, or choose it below.",
        ],
      },
    ],
    leftOut: [{ kind: "Backlinks", reason: "`[[Links]]` become plain text." }],
  },
  {
    id: "joplin",
    name: "Joplin",
    group: "notes",
    tagline: "Notebooks of Markdown notes",
    carries: ["Notes", "Notebooks as categories", "Tags", "Images", "Dates"],
    routes: [
      {
        kind: "files",
        label: "Markdown export",
        folder: true,
        accept: ["md", "zip"],
        steps: [
          "In Joplin, choose **File › Export all › MD - Markdown + Front Matter** and pick an empty folder.",
          "Choose that folder below, or drop it here.",
        ],
      },
    ],
  },
  {
    id: "mem",
    name: "Mem",
    group: "notes",
    tagline: "Notes exported as Markdown",
    carries: ["Notes"],
    routes: [
      {
        kind: "files",
        label: "Markdown export",
        accept: ["md", "zip"],
        steps: [
          "In Mem, open **Settings** and export your notes as **Markdown**.",
          "Choose the export below. Mem writes everything into one file, so it arrives as one long note.",
        ],
      },
    ],
  },

  // Meeting note takers
  {
    id: "granola",
    name: "Granola",
    group: "meetings",
    tagline: "Meeting notes, summaries and transcripts",
    keywords: ["ai notes", "meeting"],
    carries: ["Summary", "Your notes", "Transcript", "Speakers", "Timestamps", "Folders"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Granola's API comes with Business and Enterprise plans.",
        placeholder: "grn_…",
        steps: [
          "In Granola, open **Settings** and find **API keys**. Create a personal key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      {
        kind: "files",
        label: "CSV export",
        accept: ["csv", "zip", "txt", "md"],
        steps: [
          "In Granola, open **Settings › Profile** and choose **Generate CSV**. It's free on every plan, once a day.",
          "Granola emails you a link, usually within a few hours. Download the CSV.",
          "Drop the CSV here, or choose it below.",
        ],
      },
      pasteRoute("Granola (use **Copy notes** and the transcript's copy button)"),
    ],
    leftOut: [
      {
        kind: "Notes without a summary",
        reason: "Granola leaves them out of its CSV and API. Copy and paste them instead.",
      },
      recordings("Granola"),
    ],
  },
  {
    id: "anarlog",
    name: "Anarlog",
    group: "meetings",
    tagline: "Formerly Hyprnote. Read from this Mac",
    keywords: ["hyprnote", "char", "local"],
    carries: ["Summary", "Your notes", "Transcript", "Named speakers", "Timestamps", "Folders"],
    routes: [
      { kind: "local", label: "On this Mac" },
      {
        kind: "files",
        label: "Markdown export",
        accept: ["md", "zip"],
        folder: true,
        steps: [
          "In Anarlog, export your meetings as **Markdown**.",
          "Choose the exported folder below, or drop it here.",
        ],
      },
    ],
    leftOut: [
      recordings("Anarlog"),
      { kind: "Chats", reason: "Conversations with Anarlog's assistant stay there." },
    ],
  },
  {
    id: "meetily",
    name: "Meetily",
    group: "meetings",
    tagline: "Local meeting minutes",
    carries: ["Summary", "Your notes", "Transcript", "You and others", "Timestamps"],
    routes: [
      { kind: "local", label: "On this Mac" },
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Export a meeting from Meetily as **Markdown** or **Word**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
    leftOut: [recordings("Meetily")],
  },
  {
    id: "fireflies",
    name: "Fireflies",
    group: "meetings",
    tagline: "Transcripts and summaries",
    keywords: ["fireflies.ai", "fred"],
    carries: ["Summary", "Action items", "Transcript", "Speakers", "Timestamps", "Attendees"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Works on every plan. Free plans allow 500 requests a day, about 25,000 meetings.",
        steps: [
          "In Fireflies, open **Settings › Developer settings** and copy your **API key**.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        folder: true,
        steps: [
          "Open a meeting and choose **Download** for the transcript (with speaker names and timestamps) and the summary. JSON or Markdown keep the most.",
          "For everything at once, request your account data from Fireflies' help centre; they email a zip.",
          "Choose the files or zip below, or drop them here.",
        ],
      },
    ],
    leftOut: [recordings("Fireflies")],
  },
  {
    id: "fathom",
    name: "Fathom",
    group: "meetings",
    tagline: "Call notes, summaries and transcripts",
    carries: ["Summary", "Action items", "Transcript", "Speakers", "Timestamps", "Attendees"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Works on every plan. Fathom limits how fast Cilica can read, so big accounts take a few minutes.",
        steps: [
          "In Fathom, open **Settings › API Access** and create a key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      pasteRoute("Fathom (use the copy buttons on the summary and transcript)"),
    ],
    leftOut: [recordings("Fathom")],
  },
  {
    id: "otter",
    name: "Otter",
    group: "meetings",
    tagline: "Conversations and transcripts",
    keywords: ["otter.ai"],
    carries: ["Transcript", "Speakers", "Timestamps", "Summary (exported separately)"],
    routes: [
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        folder: true,
        steps: [
          "Open a conversation, choose **⋯ › Export text**, and pick **TXT** or **DOCX** with **Speaker names** and **Timestamps** on. Free plans can export TXT.",
          "On Business plans, select several conversations and choose **Export** to get one zip.",
          "Export the summary too (**Export summary**) and drop both here; Cilica pairs them by name.",
        ],
      },
      {
        kind: "key",
        label: "API key",
        plan: "Otter's API is for Enterprise workspaces.",
        steps: [
          "In Otter, open **Integrations › Developer** and create an API key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
    ],
    leftOut: [recordings("Otter")],
  },
  {
    id: "tldv",
    name: "tl;dv",
    group: "meetings",
    tagline: "Meeting recorder notes",
    keywords: ["tldv"],
    carries: ["Notes", "Transcript", "Speakers", "Timestamps", "Attendees"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "tl;dv's API comes with Pro, Business and Enterprise plans.",
        steps: [
          "In tl;dv, open **Settings** and create an API key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      pasteRoute("tl;dv (Free plans can copy the transcript)"),
    ],
    leftOut: [recordings("tl;dv")],
  },
  {
    id: "circleback",
    name: "Circleback",
    group: "meetings",
    tagline: "Meeting notes and action items",
    carries: [
      "Notes",
      "Your notes",
      "Action items",
      "Transcript",
      "Speakers",
      "Timestamps",
      "Tags",
    ],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Works on every plan, including Free (which keeps less history).",
        placeholder: "cb_…",
        steps: [
          "In Circleback, open **Settings › API keys** and create a key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
    ],
    leftOut: [recordings("Circleback")],
  },
  {
    id: "meetgeek",
    name: "MeetGeek",
    group: "meetings",
    tagline: "Meeting summaries and transcripts",
    carries: ["Summary", "Insights", "Transcript", "Speakers"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Free and Pro plans allow 100 API calls a day, about 33 meetings; run the import again each day to finish. Cilica finds your region (EU or US) by itself.",
        steps: [
          "In MeetGeek, open **Integrations › Public API** and copy your key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
    ],
    leftOut: [recordings("MeetGeek")],
  },
  {
    id: "jamie",
    name: "Jamie",
    group: "meetings",
    tagline: "Bot-free meeting notes",
    keywords: ["meetjamie"],
    carries: ["Summary", "Your notes", "Tasks", "Transcript", "Speakers"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Jamie's API comes with Pro, Team and Enterprise plans.",
        placeholder: "jk_…",
        steps: [
          "In Jamie, open **Settings** and create a personal API key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      pasteRoute("Jamie"),
    ],
    leftOut: [
      {
        kind: "Timestamps",
        reason: "Jamie's transcripts have none, so Cilica shows the lines in order without times.",
      },
    ],
  },
  {
    id: "limitless",
    name: "Limitless",
    group: "meetings",
    tagline: "Pendant and app recordings",
    keywords: ["rewind", "pendant", "lifelog"],
    carries: ["Outline", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "key",
        label: "API key",
        plan: "Limitless is winding down after Meta bought it. Import soon.",
        steps: [
          "In Limitless, open **Settings › Developer** and create an API key.",
          "Paste it below. Cilica keeps it in your keychain and checks it straight away.",
        ],
      },
      {
        kind: "files",
        label: "Data export",
        accept: ["zip", "json", "md", "txt"],
        steps: [
          "In Limitless, open **Data Exports** and request an export.",
          "Drop the file it emails you here.",
        ],
      },
    ],
    leftOut: [
      recordings("Limitless"),
      { kind: "Rewind's screen history", reason: "It's encrypted on your Mac and is not notes." },
    ],
  },
  {
    id: "read_ai",
    name: "Read AI",
    group: "meetings",
    tagline: "Meeting reports and transcripts",
    carries: ["Summary", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Downloads",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a meeting report, choose **Download**, and save the recap and the transcript.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Read AI"),
    ],
    leftOut: [recordings("Read AI")],
  },
  {
    id: "grain",
    name: "Grain",
    group: "meetings",
    tagline: "Recorded calls and notes",
    carries: ["Notes", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Markdown zip",
        accept: TRANSCRIPT_FILES,
        folder: true,
        steps: [
          "In Grain, select up to 20 meetings and choose **Download › Markdown**. Repeat for more.",
          "For one meeting, download its transcript as **VTT**, **SRT** or **DOCX**.",
          "Drop the zips or files here.",
        ],
      },
    ],
    leftOut: [recordings("Grain")],
  },
  {
    id: "sembly",
    name: "Sembly",
    group: "meetings",
    tagline: "Meeting notes and transcripts",
    carries: ["Notes", "Transcript", "Speakers"],
    routes: [
      {
        kind: "files",
        label: "Markdown export",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a meeting and download the notes and the transcript as **Markdown**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
    leftOut: [recordings("Sembly")],
  },
  {
    id: "tactiq",
    name: "Tactiq",
    group: "meetings",
    tagline: "Transcripts from the browser extension",
    carries: ["Transcript", "Speakers", "Highlights"],
    routes: [
      {
        kind: "files",
        label: "TXT downloads",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a meeting in Tactiq and choose **Download › TXT**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "krisp",
    name: "Krisp",
    group: "meetings",
    tagline: "AI meeting notes",
    carries: ["Transcript", "Speakers"],
    routes: [
      {
        kind: "files",
        label: "Transcript export",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a meeting, choose **⋯ › Export transcript**, and save the .txt.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Krisp (copy the notes too)"),
    ],
    leftOut: [recordings("Krisp")],
  },
  {
    id: "bluedot",
    name: "Bluedot",
    group: "meetings",
    tagline: "Bot-free recorder",
    carries: ["Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Transcript export",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a recording, choose **⋯ › Download transcript › TXT**.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Bluedot (copy the summary too)"),
    ],
    leftOut: [recordings("Bluedot")],
  },
  {
    id: "avoma",
    name: "Avoma",
    group: "meetings",
    tagline: "Meeting transcripts",
    carries: ["Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Transcript download",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a meeting and download its transcript as **VTT**.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Avoma (copy the notes)"),
    ],
    leftOut: [recordings("Avoma")],
  },
  {
    id: "fellow",
    name: "Fellow",
    group: "meetings",
    tagline: "Meeting notes and agendas",
    carries: ["Notes", "Agendas", "Action items"],
    routes: [
      {
        kind: "files",
        label: "Word export",
        accept: ["docx", "txt", "md"],
        steps: [
          "Open a note and choose **⋯ › Export to Word**.",
          "Choose the .docx files below, or drop them here.",
        ],
      },
      pasteRoute("Fellow"),
    ],
  },
  {
    id: "supernormal",
    name: "Supernormal",
    group: "meetings",
    tagline: "AI meeting notes",
    carries: ["Notes", "Transcript", "Speakers"],
    routes: [pasteRoute("Supernormal")],
  },
  {
    id: "notta",
    name: "Notta",
    group: "meetings",
    tagline: "Transcripts and summaries",
    carries: ["Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a recording and export it as **TXT**, **DOCX** or **SRT**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "plaud",
    name: "Plaud",
    group: "meetings",
    tagline: "NotePin, Note and Plaud Desktop",
    carries: ["Summary", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        steps: [
          "In the Plaud app, export the transcript as **TXT** or **SRT** with **Speakers** and **Timestamps** on, and the summary as **MD**.",
          "Choose both below, or drop them here.",
        ],
      },
    ],
    leftOut: [recordings("Plaud")],
  },
  {
    id: "macwhisper",
    name: "MacWhisper",
    group: "meetings",
    tagline: "Local transcriptions",
    carries: ["Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Exported files",
        accept: TRANSCRIPT_FILES,
        steps: [
          "Open a transcription and export it as **SRT**, **VTT** or **Markdown**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "superwhisper",
    name: "Superwhisper",
    group: "meetings",
    tagline: "Meeting notes on your Mac",
    carries: ["Summary", "Your notes", "Transcript"],
    routes: [
      {
        kind: "files",
        label: "Markdown",
        accept: ["md", "txt", "json"],
        folder: true,
        steps: [
          "Open **Meetings** in Superwhisper and drag a meeting's Markdown out to a folder.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Superwhisper"),
    ],
  },
  {
    id: "shadow",
    name: "Shadow",
    group: "meetings",
    tagline: "Meetings saved as Markdown",
    carries: ["Notes", "Transcript", "Speakers"],
    routes: [
      {
        kind: "files",
        label: "Shadow's folder",
        accept: ["md"],
        folder: true,
        steps: [
          "Shadow keeps each meeting as a Markdown file. Find the folder in Shadow's settings.",
          "Choose that folder below, or drop it here.",
        ],
      },
    ],
    leftOut: [recordings("Shadow")],
  },
  {
    id: "notion_meetings",
    name: "Notion AI Meeting Notes",
    group: "meetings",
    tagline: "Meeting notes recorded in Notion",
    keywords: ["notion ai"],
    carries: ["Summary", "Notes", "Transcript"],
    routes: [
      {
        kind: "files",
        label: "Page export",
        accept: ["zip", "md", "html"],
        folder: true,
        steps: [
          "Open the meeting note's **⋯** menu, choose **Export**, then **Markdown & CSV** with **Include subpages** on.",
          "Drop the zip here. If the transcript is missing from the export, copy it from Notion and use **Paste**.",
        ],
      },
      pasteRoute("Notion"),
    ],
  },
  {
    id: "voice_memos",
    name: "Voice Memos",
    group: "meetings",
    tagline: "Transcripts of your memos",
    carries: ["Transcript"],
    routes: [
      {
        kind: "paste",
        label: "Paste",
        steps: [
          "In Voice Memos, open a memo's transcript, select all of it and copy.",
          "Paste it below and give it a title.",
        ],
      },
    ],
    leftOut: [{ kind: "Audio", reason: "Memos stay in Voice Memos." }],
  },

  // Meeting apps
  {
    id: "zoom",
    name: "Zoom",
    group: "platforms",
    tagline: "AI Companion summaries and transcripts",
    carries: ["Summary", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Downloads",
        accept: ["vtt", "txt", "docx", "md", "pdf"],
        steps: [
          "Sign in at `zoom.us`, open **Recordings** and download the **Audio transcript** (.vtt).",
          "For AI Companion summaries, open **AI Companion › Meeting summaries** and export as **Word**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
    leftOut: [
      { kind: "My Notes", reason: "Zoom has no export for My Notes yet. Copy and paste them." },
    ],
  },
  {
    id: "google_meet",
    name: "Google Meet",
    group: "platforms",
    tagline: "Gemini notes and transcripts",
    keywords: ["gemini", "take notes for me"],
    carries: ["Summary", "Transcript", "Speakers"],
    routes: [
      {
        kind: "files",
        label: "Docs from Drive",
        accept: DOC_FILES,
        folder: true,
        steps: [
          "In Google Drive, open the **Google Meet** folder (older meetings are in **Meet Recordings**).",
          "Open the notes or transcript Doc and choose **File › Download › Markdown (.md)**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "teams",
    name: "Microsoft Teams",
    group: "platforms",
    tagline: "Meeting transcripts and recaps",
    keywords: ["microsoft", "copilot"],
    carries: ["Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Transcript download",
        accept: ["vtt", "docx", "txt"],
        steps: [
          "Open the meeting's **Recap** tab, then **Transcript**, and choose **Download › .vtt**.",
          "Choose the files below, or drop them here.",
        ],
      },
      pasteRoute("Teams (copy Copilot's recap)"),
    ],
    leftOut: [{ kind: "Loop notes", reason: "Loop has no export. Copy and paste them." }],
  },
  {
    id: "webex",
    name: "Webex",
    group: "platforms",
    tagline: "AI Assistant summaries and transcripts",
    carries: ["Summary", "Transcript", "Speakers", "Timestamps"],
    routes: [
      {
        kind: "files",
        label: "Meeting content",
        accept: ["vtt", "txt", "docx", "zip"],
        steps: [
          "In the Webex App, open the meeting and choose **Download meeting content**.",
          "Choose the files below, or drop them here.",
        ],
      },
    ],
  },
  {
    id: "slack",
    name: "Slack huddles",
    group: "platforms",
    tagline: "Huddle notes in canvases",
    carries: ["Notes"],
    routes: [pasteRoute("the huddle's canvas")],
  },
  {
    id: "microsoft_loop",
    name: "Microsoft Loop",
    group: "platforms",
    tagline: "Loop pages and meeting notes",
    carries: ["Notes"],
    routes: [pasteRoute("Loop")],
    leftOut: [{ kind: "Export", reason: "Loop has no export, so copying is the only way out." }],
  },

  // Anything else
  {
    id: "files",
    name: "Other files",
    group: "other",
    tagline: "Markdown, Word, text, transcripts or a zip",
    keywords: ["markdown", "docx", "txt", "vtt", "srt", "csv", "folder", "zip"],
    carries: ["Notes", "Transcripts", "Folders as categories", "Images"],
    routes: [
      {
        kind: "files",
        label: "Files or folder",
        accept: [
          "md",
          "markdown",
          "txt",
          "docx",
          "html",
          "vtt",
          "srt",
          "csv",
          "json",
          "enex",
          "zip",
        ],
        folder: true,
        steps: [
          "Cilica reads Markdown, Word, text, HTML, transcripts (VTT, SRT, text with speaker names), CSV, Evernote and zips of any of these.",
          "Choose files or a folder below, or drop them here.",
        ],
      },
      pasteRoute("the other app"),
    ],
  },
];

export const PROVIDERS: readonly ImportProvider[] = CATALOGUE.map((p) => ({
  ...p,
  logo: logoFor(p.id),
  glyph: p.id === "files" ? FolderOpen : undefined,
}));

export function providerById(id: string): ImportProvider | undefined {
  return PROVIDERS.find((p) => p.id === id);
}

/** Providers in display order, grouped. */
export function groupedProviders(): { group: ProviderGroup; providers: ImportProvider[] }[] {
  return (Object.keys(GROUP_LABELS) as ProviderGroup[]).map((group) => ({
    group,
    providers: PROVIDERS.filter((p) => p.group === group),
  }));
}

/**
 * A citation in one of Margin's answers: the note's title, and when the note still has its
 * transcript, the moment as a time chip that opens the transcript there (parking the playhead
 * if the recording is still on disk). No transcript, no chip: the title alone opens the note.
 * The transcript is only fetched for notes Margin cited with a time, and is cached.
 */
import { Play } from "lucide-react";
import { openNote } from "@/app/router";
import { useTranscript } from "@/features/transcript/hooks";
import { timecodeMs } from "@/lib/markdown";
import { Icon } from "@/ui";

export function Receipt({
  noteId,
  title,
  time,
}: {
  noteId: string;
  title: string;
  time: string | null;
}) {
  const at = time ? timecodeMs(time) : null;
  const { data: transcript } = useTranscript(noteId, at !== null);
  const last = transcript?.segments[transcript.segments.length - 1];
  // The moment has to exist in what is still stored.
  const found = at !== null && last !== undefined && at <= last.end_ms + 1000;
  return (
    <>
      <button type="button" className="nt-cite" onClick={() => openNote(noteId)}>
        “{title}”
      </button>
      {found && (
        <button
          type="button"
          className="nt-receipt"
          title={
            transcript?.has_audio
              ? "Open the recording at this moment"
              : "Open the transcript at this moment"
          }
          onClick={() => openNote(noteId, { panel: "transcript", atMs: at })}
        >
          <Icon icon={Play} size="sm" />
          {time}
        </button>
      )}
    </>
  );
}

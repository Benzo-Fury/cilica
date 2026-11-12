/**
 * Shown when no note is open; on a fresh install it is the welcome. Otherwise one face: the one kept
 * in Settings › Appearance, or by default one picked at random each visit from those the week's
 * meetings can fill: the word of the week, a line someone said, who did the talking, how fast you speak, the longest silence.
 * The doodle pad shows only when kept or when nothing else can.
 */
import { Shuffle } from "lucide-react";
import { useEffect, useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { useNotes } from "@/features/notes/hooks";
import { useSettings } from "@/features/settings/hooks";
import { CanvasBand } from "@/features/shell/CanvasBand";
import { useIntro } from "@/features/shell/intro";
import { Button, EmptyState, GlanceFooter, ScrollArea, ViewLayer } from "@/ui";
import { FaceView } from "./faces";
import { availableFaces, type GlanceFace, openingFace, pickFace, useHomeGlance } from "./glance";

/** The face shown last, so the next visit shows a different one. */
let lastFace: GlanceFace | null = null;

export function HomeView() {
  const { data: notes, isLoading } = useNotes();
  const fresh = !isLoading && (notes?.length ?? 0) === 0;
  const glance = useHomeGlance(!isLoading && !fresh);
  const [roll] = useState(Math.random);
  const faces = availableFaces(glance.data);
  const { data: settings } = useSettings();
  const kept = settings?.appearance.home_face;
  const ready = !isLoading && (fresh || (!glance.isPending && kept !== undefined));
  // Settle the face once the data is in; keep it for the rest of this visit.
  const [face, setFace] = useState<GlanceFace | null>(null);
  const [settledFor, setSettledFor] = useState(kept);
  if (settledFor !== kept) {
    // The choice in Settings changed: settle again.
    setSettledFor(kept);
    setFace(null);
  }
  useEffect(() => {
    if (!ready || fresh || face || kept === undefined) return;
    const f = openingFace(faces, kept, lastFace, roll);
    lastFace = f;
    setFace(f);
  }, [ready, fresh, face, faces, roll, kept]);
  // The launch intro holds on this page from the moment there is something to read.
  const showFace = useIntro((s) => s.showFace);
  useEffect(() => {
    if (fresh || face) showFace();
  }, [fresh, face, showFace]);
  // Another face from what is available, never the one on screen.
  const next = () => {
    const f = pickFace(faces, face, Math.random());
    lastFace = f;
    setFace(f);
  };

  return (
    <ViewLayer>
      <CanvasBand />
      <ScrollArea className="nt-home">
        {fresh && (
          <EmptyState
            art={false}
            title="No notes yet"
            tip={<ShortcutTip action="recording.toggle">Start recording from anywhere</ShortcutTip>}
          />
        )}
        {face && <FaceView face={face} glance={glance.data} />}
        {face && faces.some((f) => f !== face) && (
          <GlanceFooter>
            <Button variant="ghost" size="sm" icon={Shuffle} onClick={next}>
              Show something else
            </Button>
          </GlanceFooter>
        )}
      </ScrollArea>
    </ViewLayer>
  );
}

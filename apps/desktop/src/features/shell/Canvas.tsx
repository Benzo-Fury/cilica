/**
 * The canvas content for the current view. Views cross-fade with a short slide (`springs.gentle`);
 * a note opened from the sidebar slides in from the left. The recorder capsule floats above.
 * At launch the first view rises in on its own while the intro holds (intro.ts).
 */
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import type { ReactNode } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { useRouter, type View } from "@/app/router";
import { ActivityView } from "@/features/activity/ActivityView";
import { MarginBar } from "@/features/assistant/MarginBar";
import { useMarginPlacement } from "@/features/assistant/placement";
import { HomeView } from "@/features/home/HomeView";
import { GraphView } from "@/features/links/GraphView";
import { NoteView } from "@/features/note/NoteView";
import { PeopleView } from "@/features/people/PeopleView";
import { RecorderCapsule } from "@/features/recorder/RecorderCapsule";
import { SettingsView } from "@/features/settings/SettingsView";
import { SwearJarLayer } from "@/features/swearjar/SwearJarLayer";
import { TasksView } from "@/features/tasks/TasksView";
import { TrashView } from "@/features/trash/TrashView";
import { LAUNCH_SLOWDOWN, useIntroHolding } from "./intro";

function viewKey(view: View): string {
  // A task report is its own page; the task list is another.
  if (view.name === "tasks") return view.taskRunId ? `tasks:${view.taskRunId}` : "tasks";
  return view.name === "note" ? `note:${view.noteId}` : view.name;
}

function renderView(view: View): ReactNode {
  switch (view.name) {
    case "home":
      return <HomeView />;
    case "note":
      return <NoteView noteId={view.noteId} />;
    case "trash":
      return <TrashView />;
    case "settings":
      return <SettingsView page={view.page} />;
    case "people":
      return <PeopleView />;
    case "graph":
      return <GraphView focus={view.focus} />;
    case "tasks":
      return <TasksView taskRunId={view.taskRunId} />;
    case "activity":
      return <ActivityView />;
  }
}

export function Canvas() {
  const view = useRouter((s) => s.view);
  const reduced = useReducedMotion();
  const fromLeft = view.name === "note";
  const floatingBar = useMarginPlacement() === "floating_bar";
  const launching = useIntroHolding();
  return (
    <>
      {/* Only the launch view animates on mount: it rises in while the intro holds. */}
      <AnimatePresence initial={launching} mode="sync">
        <motion.div
          key={viewKey(view)}
          className="nt-view"
          initial={
            launching
              ? { opacity: 0, x: 0, y: reduced ? 0 : 12, scale: reduced ? 1 : 0.985 }
              : { opacity: 0, x: reduced ? 0 : fromLeft ? -8 : 8 }
          }
          animate={{
            opacity: 1,
            x: 0,
            y: 0,
            scale: 1,
            transition: reduced
              ? reducedTransition
              : launching
                ? {
                    ...springs.gentle,
                    visualDuration: springs.gentle.visualDuration * LAUNCH_SLOWDOWN,
                  }
                : springs.gentle,
          }}
          exit={{ opacity: 0, transition: tweens.exit }}
        >
          {renderView(view)}
        </motion.div>
      </AnimatePresence>
      <RecorderCapsule />
      <SwearJarLayer />
      {floatingBar && <MarginBar />}
    </>
  );
}

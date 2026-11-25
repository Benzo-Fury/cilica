/**
 * Open notes as tabs (Settings › General › Tabs, off by default). The bar order is remembered
 * across launches; Cilica still opens on Home, with the tabs waiting along the bottom.
 *
 * Notes open through the router from everywhere (sidebar, palette, links, the graph), so the tabs
 * follow the router rather than every caller knowing about them (see `follow` in model.ts). The
 * few places that open a note to work on it (a new note, a recording) say so with
 * `openNoteToWork`, and saving an edit or a rename calls `keepTab`.
 */
import { useEffect } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { navigate, openNote, useRouter, type View } from "@/app/router";
import { addTab, type Bar, closeTab, direction, follow, keep, prune } from "./model";

interface TabsState extends Bar {
  /** Mirrors the setting; while off, the router is not followed and no bar shows. */
  enabled: boolean;
  /** Which way the last switch moved along the bar, so the page can slide in from that side. */
  direction: -1 | 0 | 1;
  /** A note about to open to be worked on, so the router follower gives it a tab of its own. */
  working: string | null;
}

export const useTabs = create<TabsState>()(
  persist(
    (): TabsState => ({ enabled: false, tabs: [], preview: null, direction: 0, working: null }),
    {
      name: "cilica.tabs",
      version: 2,
      partialize: ({ tabs, preview }) => ({ tabs, preview }),
      // v1 kept only the order; those tabs were all opened deliberately, so none is a preview.
      migrate: (persisted) => ({
        tabs: (persisted as { tabs?: string[] } | undefined)?.tabs ?? [],
        preview: null,
      }),
    },
  ),
);

const noteId = (view: View): string | null => (view.name === "note" ? view.noteId : null);
const barOf = ({ tabs, preview }: TabsState): Bar => ({ tabs, preview });

/** Open a note in a tab of its own after the current one, staying where you are. */
export function openInNewTab(id: string) {
  const state = useTabs.getState();
  if (!state.enabled) return openNote(id);
  const current = noteId(useRouter.getState().view);
  // From anywhere but a tab, the new tab is also where you go.
  if (!current || !state.tabs.includes(current)) return openNoteToWork(id);
  useTabs.setState(addTab(barOf(state), current, id));
}

/**
 * Open a note to work on it: a note just created, or one being recorded into. It gets a tab of
 * its own rather than the preview tab, so the note you were in stays open beside it.
 */
export function openNoteToWork(id: string, options?: Parameters<typeof openNote>[1]) {
  if (useTabs.getState().enabled) useTabs.setState({ working: id });
  openNote(id, options);
  // Already on screen: the router didn't move, so keep it here.
  if (useTabs.getState().working === id) {
    useTabs.setState((s) => ({ ...keep(barOf(s), id), working: null }));
  }
}

/** The note was changed (written in, renamed): its tab stays when you open another note. */
export function keepTab(id: string) {
  const state = useTabs.getState();
  if (state.preview === id) useTabs.setState(keep(barOf(state), id));
}

/** Close a tab. Closing the note on screen shows its neighbour, or Home after the last one. */
export function closeNoteTab(id: string) {
  const state = useTabs.getState();
  const current = noteId(useRouter.getState().view);
  const result = closeTab(barOf(state), id, current);
  const going = id === current;
  useTabs.setState({
    ...result.bar,
    direction: going && result.next ? direction(state.tabs, id, result.next) : 0,
  });
  if (!going) return;
  if (result.next) openNote(result.next);
  else navigate({ name: "home" });
}

/** Drop tabs for notes that are gone (deleted, or in Trash). */
export function pruneTabs(exists: (id: string) => boolean) {
  const state = useTabs.getState();
  const next = prune(barOf(state), exists);
  if (next.tabs.length !== state.tabs.length) useTabs.setState(next);
}

/**
 * Keep the tabs in step with the router while tabs are on. Turning tabs off closes them all, so
 * turning them back on starts from the note on screen.
 */
export function useTabsFollowRouter(enabled: boolean | undefined) {
  useEffect(() => {
    // Settings still loading: leave the remembered tabs alone.
    if (enabled === undefined) return;
    if (!enabled) {
      // Only an actual switch-off forgets the tabs, not a launch with tabs off.
      if (useTabs.getState().enabled) useTabs.setState({ enabled, tabs: [], preview: null });
      return;
    }
    const state = useTabs.getState();
    useTabs.setState({ enabled, ...follow(barOf(state), null, noteId(useRouter.getState().view)) });
    return useRouter.subscribe((now, previous) => {
      const from = noteId(previous.view);
      const to = noteId(now.view);
      if (from === to) return;
      const tabs = useTabs.getState();
      const opening = to && tabs.working === to ? "work" : "look";
      useTabs.setState({
        ...follow(barOf(tabs), from, to, opening),
        direction: direction(tabs.tabs, from, to),
        working: null,
      });
    });
  }, [enabled]);
}

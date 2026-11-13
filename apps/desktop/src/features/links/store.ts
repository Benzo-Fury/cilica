/** The graph's few options, remembered between launches. */
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { GraphFilter } from "./graphModel";

interface GraphOptions extends GraphFilter {
  set: (patch: Partial<GraphFilter>) => void;
}

export const useGraphOptions = create<GraphOptions>()(
  persist(
    (set) => ({
      people: true,
      tags: false,
      missing: true,
      colorBy: "type",
      set: (patch) => set(patch),
    }),
    {
      name: "cilica.graph",
      partialize: ({ people, tags, missing, colorBy }) => ({ people, tags, missing, colorBy }),
    },
  ),
);

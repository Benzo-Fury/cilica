import { isMac } from "@/app/platform";

/** What every step gets from the setup window. */
export interface StepProps {
  status: "done" | "active";
  /** Left with Skip; the line says so. */
  skipped: boolean;
  /** Move on. `skipped` when nothing was set up here. */
  onNext: (skipped: boolean) => void;
  onReopen: () => void;
}

/** Privacy copy names the device people are holding. */
export const THIS_DEVICE = isMac ? "this Mac" : "this computer";

/** 1,516 */
export const formatCount = (n: number) => n.toLocaleString("en-GB");

/** 0..1 for a download, or null while its size is unknown. */
export function fraction(
  download: { downloaded: number | null; total: number | null } | undefined,
) {
  if (!download?.total) return null;
  return Math.min(1, (download.downloaded ?? 0) / download.total);
}

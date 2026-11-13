/**
 * "A line from your week" for the home screen. Cilica writes it from recent note titles; it
 * costs one small model call, so the answer is kept for the day and asked for at most once.
 */
import { useQuery } from "@tanstack/react-query";
import { api } from "@/ipc/client";

const KEY = "cilica.weekLine";

/** The local calendar day, so the line changes overnight and not mid-afternoon. */
export function dayKey(now = new Date()): string {
  return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}`;
}

export function readWeekLine(day: string): string | null {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null") as {
      day?: string;
      line?: string;
    } | null;
    return saved?.day === day && typeof saved.line === "string" ? saved.line : null;
  } catch {
    return null;
  }
}

export function useWeekLine(enabled: boolean): string | null {
  const day = dayKey();
  const { data } = useQuery({
    queryKey: ["week_line", day],
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
    queryFn: async () => {
      const saved = readWeekLine(day);
      if (saved) return saved;
      const line = await api.getWeekLine().catch(() => null);
      if (line) {
        try {
          localStorage.setItem(KEY, JSON.stringify({ day, line }));
        } catch {
          // Without storage the line is simply asked for again next launch.
        }
      }
      return line;
    },
  });
  return data ?? null;
}

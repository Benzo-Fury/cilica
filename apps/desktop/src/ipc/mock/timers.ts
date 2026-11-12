/** A registry of timeouts and intervals that can be cleared together. */
export class Timers {
  private readonly timeouts = new Set<ReturnType<typeof setTimeout>>();
  private readonly intervals = new Set<ReturnType<typeof setInterval>>();

  later(ms: number, fn: () => void): ReturnType<typeof setTimeout> {
    const handle = setTimeout(() => {
      this.timeouts.delete(handle);
      fn();
    }, ms);
    this.timeouts.add(handle);
    return handle;
  }

  every(ms: number, fn: () => void): ReturnType<typeof setInterval> {
    const handle = setInterval(fn, ms);
    this.intervals.add(handle);
    return handle;
  }

  stop(handle: ReturnType<typeof setInterval> | ReturnType<typeof setTimeout> | null): void {
    if (handle === null) return;
    clearTimeout(handle);
    clearInterval(handle);
    this.timeouts.delete(handle);
    this.intervals.delete(handle);
  }

  clear(): void {
    for (const t of this.timeouts) clearTimeout(t);
    for (const i of this.intervals) clearInterval(i);
    this.timeouts.clear();
    this.intervals.clear();
  }
}

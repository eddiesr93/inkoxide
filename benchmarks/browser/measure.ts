// What both pages measure, the same way: the first invoice from the moment the page started
// loading (the library, its engine and fonts included), then the median of warm renders of each
// document, and how long the page's main thread was blocked meanwhile (Chromium reports long
// tasks; other browsers report none).

export interface Renders {
  invoice(): Promise<Uint8Array>;
  report(): Promise<Uint8Array>;
}

export interface Result {
  /** Milliseconds from navigation start to the first invoice's PDF. */
  coldMs: number;
  warm: Record<keyof Renders, { ms: number; pages: number; longestTaskMs: number | null }>;
}

const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;

/** The page count of a PDF: `/Count` of its page tree root, which both libraries write as text. */
function pages(pdf: Uint8Array): number {
  const text = new TextDecoder('latin1').decode(pdf);
  const counts = [
    ...text.matchAll(/\/Type\s*\/Pages\b[^>]*?\/Count\s+(\d+)|\/Count\s+(\d+)[^>]*?\/Type\s*\/Pages\b/g),
  ];
  return Math.max(0, ...counts.map((m) => Number(m[1] ?? m[2])));
}

/** Runs the measurements, and leaves the result in `globalThis.bench` for the runner. */
export async function measure(renders: Renders): Promise<void> {
  const params = new URLSearchParams(location.search);
  const runs = Number(params.get('runs') ?? 20);
  const status = document.querySelector('#status') as HTMLElement;
  try {
    await renders.invoice();
    const coldMs = performance.now();

    const longTasks: number[] = [];
    const supported = PerformanceObserver.supportedEntryTypes.includes('longtask');
    const observer = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) longTasks.push(e.duration);
    });
    if (supported) observer.observe({ type: 'longtask' });

    const warm = {} as Result['warm'];
    for (const doc of ['invoice', 'report'] as const) {
      const n = doc === 'report' ? Math.min(runs, 10) : runs;
      let pdf = await renders[doc]();
      longTasks.length = 0;
      const times: number[] = [];
      for (let i = 0; i < n; i++) {
        const t = performance.now();
        pdf = await renders[doc]();
        times.push(performance.now() - t);
      }
      // Let the observer deliver the last entries.
      await new Promise((resolve) => setTimeout(resolve, 50));
      warm[doc] = {
        ms: median(times),
        pages: pages(pdf),
        longestTaskMs: supported ? Math.max(0, ...longTasks) : null,
      };
    }
    observer.disconnect();
    (globalThis as { bench?: Result }).bench = { coldMs, warm };
    status.textContent = JSON.stringify({ coldMs, warm });
  } catch (e) {
    (globalThis as { bench?: { error: string } }).bench = { error: String((e as Error)?.stack ?? e) };
    throw e;
  }
}

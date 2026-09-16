import { m } from "@/i18n";

/** Below this a span reads better in whole milliseconds than in tenths of a second. */
const SECOND_MS = 1000;

/**
 * A measured span as the panel prints it: `420 ms`, or `3.7 s` once past a second.
 *
 * Every number the panel shows is something the machine measured rather than a
 * constant, so the unit follows the value instead of being fixed by the slot.
 */
export function formatSpan(ms: number): string {
  if (ms < SECOND_MS) return m.champ_select_span_ms({ value: Math.round(ms) });
  return m.champ_select_span_s({ value: (ms / SECOND_MS).toFixed(1) });
}

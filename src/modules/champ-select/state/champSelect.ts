import { create } from "zustand";

import type { ChampSelectView, SwapReport } from "@/lib/tauri";

interface ChampSelectStore {
  /** The local player's part of champion select, while there is one. */
  view: ChampSelectView | null;
  /** What the scheduler last said about a swap, until the next one. */
  lastReport: SwapReport | null;
  /** Whether the reader has closed the panel for this champion select. */
  dismissed: boolean;
  started: (view: ChampSelectView) => void;
  changed: (view: ChampSelectView) => void;
  ended: () => void;
  reported: (report: SwapReport) => void;
  dismiss: () => void;
}

/**
 * What champion select is doing, pushed from the backend.
 *
 * Client state rather than a query: nothing here is fetched, it arrives as
 * events, and the one read that exists is the catch-up a mounting webview does.
 *
 * Only what the local player is doing. The backend reduces a session about ten
 * players to one cell before it crosses, so nothing about anyone else is here
 * to leak into a screen.
 */
export const useChampSelectStore = create<ChampSelectStore>()((set) => ({
  view: null,
  lastReport: null,
  dismissed: false,

  started: (view) => set({ view, lastReport: null, dismissed: false }),
  changed: (view) => set({ view }),

  // The report goes with the select it was about. Carrying it into the next one
  // would explain a swap the reader is no longer in.
  ended: () => set({ view: null, lastReport: null, dismissed: false }),

  reported: (report) => set({ lastReport: report }),

  /* Closing the panel is about this select, not about the feature: the next one
     raises it again, and the scheduler keeps swapping either way. */
  dismiss: () => set({ dismissed: true }),
}));

/** The champion the swap is about: the lock, or the hover before it. */
export function championInPlay(view: ChampSelectView | null): number | null {
  if (!view) return null;
  return view.lockedChampionId ?? view.hoveredChampionId ?? null;
}

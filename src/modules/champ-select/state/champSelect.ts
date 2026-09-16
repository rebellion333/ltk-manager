import { create } from "zustand";

import type { ChampSelectView, SwapReport } from "@/lib/tauri";

interface ChampSelectStore {
  /** The local player's part of champion select, while there is one. */
  view: ChampSelectView | null;
  /** What the scheduler last said about a swap, until the next one. */
  lastReport: SwapReport | null;
  /** The champion the reader closed the panel on, while it is still in play. */
  dismissedFor: number | null;
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
  dismissedFor: null,

  started: (view) => set({ view, lastReport: null, dismissedFor: null }),

  /* A dismissal lasts while the pick it was about does. The backend raises the
     window once per champion, so a dismissal that outlived the champion would
     leave the window coming forward over a panel that refuses to open - which
     is what a reader sees as the app taking the screen for nothing. */
  changed: (view) =>
    set((state) => ({
      view,
      dismissedFor: championInPlay(view) === state.dismissedFor ? state.dismissedFor : null,
    })),

  // The report goes with the select it was about. Carrying it into the next one
  // would explain a swap the reader is no longer in.
  ended: () => set({ view: null, lastReport: null, dismissedFor: null }),

  reported: (report) => set({ lastReport: report }),

  /* Closing the panel is about this pick, not about the feature: another
     champion raises it again, and the scheduler keeps swapping either way. */
  dismiss: () => set((state) => ({ dismissedFor: championInPlay(state.view) })),
}));

/** The champion the swap is about: the lock, or the hover before it. */
export function championInPlay(view: ChampSelectView | null): number | null {
  if (!view) return null;
  return view.lockedChampionId ?? view.hoveredChampionId ?? null;
}

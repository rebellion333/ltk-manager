import { queryOptions } from "@tanstack/react-query";

import {
  api,
  type AppError,
  type ChampionPreference,
  type ChampionMods,
  type ChampionSummary,
  type ChampSelectStatus,
} from "@/lib/tauri";
import { queryFn } from "@/utils/query";

import { champSelectKeys } from "./keys";

/** What the scheduler is planning, and what the profile wants per champion. */
export const champSelectQueries = {
  /* Events carry every change, so this is only what a webview that has just
     mounted needs to catch up on. */
  status: () =>
    queryOptions<ChampSelectStatus, AppError>({
      queryKey: champSelectKeys.status(),
      queryFn: queryFn(api.champSelect.status),
      staleTime: Infinity,
      refetchOnWindowFocus: false,
    }),

  preferences: () =>
    queryOptions<Record<string, ChampionPreference>, AppError>({
      queryKey: champSelectKeys.preferences(),
      queryFn: queryFn(api.champSelect.preferences),
      staleTime: Infinity,
    }),

  /* The mods the reader keeps within reach, per champion, in their order. A
     favourite is where a mod sits in a list and never what a champion applies,
     so this is read beside the preferences and never instead of them. */
  favorites: () =>
    queryOptions<Record<string, string[]>, AppError>({
      queryKey: champSelectKeys.favorites(),
      queryFn: queryFn(api.champSelect.favorites),
      staleTime: Infinity,
    }),

  /* Champion select speaks in numeric ids, and everything else here speaks in
     aliases. This is the join, and it only changes when a patch adds a
     champion, so it is read once and held.

     `enabled` is off outside champion select rather than the whole roster
     riding on the boot path for a screen that may never open. */
  /* Every champion the library holds a mod for. Read by the champions page,
     which is the surface that exists outside a champion select. */
  withMods: () =>
    queryOptions<ChampionMods[], AppError>({
      queryKey: champSelectKeys.withMods(),
      queryFn: queryFn(api.champSelect.withMods),
      staleTime: Infinity,
    }),

  roster: (enabled: boolean) =>
    queryOptions<ChampionSummary[], AppError>({
      queryKey: champSelectKeys.roster(),
      queryFn: queryFn(api.champSelect.roster),
      enabled,
      staleTime: Infinity,
    }),

  /* The mods that apply to one champion. Keyed on the champion, so switching
     picks reads a fresh list and going back to one reads the cache. */
  modsFor: (champion: string | null) =>
    queryOptions<string[], AppError>({
      queryKey: champSelectKeys.mods(champion ?? ""),
      queryFn: queryFn(() => api.champSelect.modsForChampion(champion ?? "")),
      enabled: champion !== null,
      staleTime: Infinity,
    }),
} as const;

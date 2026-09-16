import { useMutation, useQueryClient } from "@tanstack/react-query";

import { api, type AppError } from "@/lib/tauri";
import { libraryKeys } from "@/modules/library";
import { unwrapForQuery } from "@/utils/query";

import { champSelectKeys } from "./keys";

export interface SetChampionPreferenceVariables {
  /** The champion's alias, which is what the library joins mods on. */
  champion: string;
  /** The mod the champion applies, or `null` to leave it with none. */
  modId: string | null;
}

/**
 * Choose the mod a champion applies, and rebuild the overlay for it.
 *
 * Not optimistic, unlike the library's own toggle: the write moves the enabled
 * set *and* rebuilds, so the moment it answers is the moment the overlay
 * actually carries the mod. Showing it applied before that would be showing a
 * game state that is not there yet, which is the one thing champion select
 * cannot afford to be wrong about.
 */
export function useSetChampionPreference() {
  const client = useQueryClient();

  return useMutation<null, AppError, SetChampionPreferenceVariables>({
    mutationFn: async ({ champion, modId }) =>
      unwrapForQuery(await api.champSelect.setPreference(champion, modId)),
    onSettled: () => {
      client.invalidateQueries({ queryKey: champSelectKeys.all });
      // The enabled set moved, so the library list is stale either way.
      client.invalidateQueries({ queryKey: libraryKeys.mods() });
    },
  });
}

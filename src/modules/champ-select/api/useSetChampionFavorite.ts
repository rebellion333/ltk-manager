import { useMutation, useQueryClient } from "@tanstack/react-query";

import { api, type AppError } from "@/lib/tauri";
import { unwrapForQuery } from "@/utils/query";

import { champSelectKeys } from "./keys";

export interface SetChampionFavoriteVariables {
  /** The champion's alias, which is what the library joins mods on. */
  champion: string;
  modId: string;
  favorite: boolean;
}

/**
 * Mark a mod as one of a champion's favourites, or unmark it.
 *
 * Optimistic, unlike choosing what a champion applies: this moves a row in a
 * list and nothing else. No overlay is rebuilt, no mod is enabled or disabled,
 * and being wrong for one frame costs a star drawn filled that empties again.
 * The other write cannot afford that, because it is about what the game loads.
 */
export function useSetChampionFavorite() {
  const client = useQueryClient();

  return useMutation<null, AppError, SetChampionFavoriteVariables, Record<string, string[]>>({
    mutationFn: async ({ champion, modId, favorite }) =>
      unwrapForQuery(await api.champSelect.setFavorite(champion, modId, favorite)),

    onMutate: async ({ champion, modId, favorite }) => {
      await client.cancelQueries({ queryKey: champSelectKeys.favorites() });
      const previous = client.getQueryData<Record<string, string[]>>(champSelectKeys.favorites());

      client.setQueryData<Record<string, string[]>>(champSelectKeys.favorites(), (old) => {
        const next = { ...(old ?? {}) };
        const marked = next[champion] ?? [];
        if (!favorite) {
          const kept = marked.filter((id) => id !== modId);
          /* The champion goes with its last favourite, so the optimistic shape
             is the one the backend answers with. */
          if (kept.length === 0) delete next[champion];
          else next[champion] = kept;
          return next;
        }
        if (!marked.includes(modId)) next[champion] = [...marked, modId];
        return next;
      });

      return previous ?? {};
    },

    onError: (_error, _variables, previous) => {
      if (previous) client.setQueryData(champSelectKeys.favorites(), previous);
    },

    onSettled: () => {
      client.invalidateQueries({ queryKey: champSelectKeys.favorites() });
    },
  });
}

import { useQueryClient } from "@tanstack/react-query";

import type { ChampSelectView, SwapReport } from "@/lib/tauri";
import { useTauriEvent } from "@/lib/useTauriEvent";
import { libraryKeys } from "@/modules/library";

import { useChampSelectStore } from "../state/champSelect";
import { champSelectKeys } from "./keys";

/**
 * Follow champion select and what the scheduler makes of it.
 *
 * Mounted once for the app rather than beside the panel: a champion select can
 * begin while the reader is anywhere, and the panel raises itself off this
 * store rather than the other way round.
 */
export function useChampSelectEvents() {
  const queryClient = useQueryClient();
  const started = useChampSelectStore((state) => state.started);
  const changed = useChampSelectStore((state) => state.changed);
  const ended = useChampSelectStore((state) => state.ended);
  const reported = useChampSelectStore((state) => state.reported);

  useTauriEvent<ChampSelectView>("champ-select-started", started);
  useTauriEvent<ChampSelectView>("champ-select-changed", changed);
  useTauriEvent<null>("champ-select-ended", ended);

  useTauriEvent<SwapReport>("champ-select-swap", (report) => {
    reported(report);
    // The scheduler just moved the enabled set and rebuilt, so anything drawn
    // from either is stale.
    void queryClient.invalidateQueries({ queryKey: champSelectKeys.all });
    void queryClient.invalidateQueries({ queryKey: libraryKeys.mods() });
  });
}

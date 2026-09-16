import { ChampionsBrowser } from "@/modules/champ-select";

/**
 * Where a reader sets up what each champion applies, away from the clock.
 *
 * A page rather than a dialog, because this is the half of champion select
 * that is not urgent: the panel exists to be answered in seconds, and this
 * exists to be sat with.
 */
export function Champions() {
  return (
    <div className="h-full bg-surface-950">
      <ChampionsBrowser />
    </div>
  );
}

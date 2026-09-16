import { CheckIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";

import { m } from "@/i18n";
import { twMerge } from "@/utils";

import { champSelectQueries } from "../api";

interface ChampionBenchProps {
  /** The bench as the client published it, which may be empty. */
  championIds: number[];
  /** Whether a champion can still be taken from it. */
  open: boolean;
}

/**
 * The ARAM bench, and which of it is already set up.
 *
 * ARAM has the longest finalization of the four modes and the shortest usable
 * budget - 9.9 s against draft's 31 - because the bench lets the champion
 * change until the phase ends. The reader is choosing between the champion they
 * rolled and five others, under that clock, and which of the five they have a
 * mod set for is part of that choice.
 *
 * Read-only. Taking a champion off the bench is an action on the account, which
 * this app does not take; it reads the client and never drives it.
 *
 * The ticked ones are the ones that need nothing: a champion with a preference
 * swaps itself when it comes into play. One with mods but no preference is not
 * ticked, because under this clock "you could pick something" is not the same
 * promise as "it is handled".
 */
export function ChampionBench({ championIds, open }: ChampionBenchProps) {
  const roster = useQuery(champSelectQueries.roster(championIds.length > 0));
  const preferences = useQuery(champSelectQueries.preferences());

  /* The client keeps publishing the bench through GAME_STARTING, when nothing
     can be taken from it any more. `canStillChange` is what knows that, so the
     two are read together rather than the list being trusted alone. */
  if (!open || championIds.length === 0) return null;

  const named = championIds.map((id) => {
    const champion = roster.data?.find((entry) => entry.id === id);
    const alias = champion?.alias;
    return {
      id,
      name: champion?.name ?? String(id),
      ready: alias !== undefined && preferences.data?.[alias] !== undefined,
    };
  });

  return (
    <div data-ui="ChampionBench" className="flex flex-col gap-1.5 select-none">
      <p className="text-fine text-surface-400">{m.champ_select_bench_label()}</p>
      <div className="flex flex-wrap gap-1.5">
        {named.map((champion) => (
          <BenchChip key={champion.id} name={champion.name} ready={champion.ready} />
        ))}
      </div>
      <p className="text-fine text-surface-400">{m.champ_select_bench_hint()}</p>
    </div>
  );
}

interface BenchChipProps {
  name: string;
  ready: boolean;
}

/**
 * One champion on the bench, and whether it is handled.
 *
 * The tick is the whole signal, so it is also the label: read aloud, both chips
 * would otherwise be a champion's name and nothing else.
 */
function BenchChip({ name, ready }: BenchChipProps) {
  const label = ready
    ? m.champ_select_bench_ready_label({ champion: name })
    : m.champ_select_bench_unset_label({ champion: name });

  return (
    <span
      aria-label={label}
      className={twMerge(
        "inline-flex items-center gap-1 rounded-md px-2 py-1 text-fine",
        "bg-surface-700/60 text-surface-300",
        ready && "bg-accent-500/12 text-accent-300",
      )}
    >
      {ready && <CheckIcon className="h-3 w-3 shrink-0" weight="bold" />}
      {name}
    </span>
  );
}

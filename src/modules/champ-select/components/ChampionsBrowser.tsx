import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { EmptyState, Field, Spinner } from "@/components";
import { m } from "@/i18n";
import type { ChampionMods } from "@/lib/tauri";
import { twMerge } from "@/utils";

import { champSelectQueries } from "../api";
import { ChampionModList } from "./ChampionModList";

/**
 * Every champion the library holds a mod for, and what each one applies.
 *
 * The other half of champion select. The panel is where a choice is made under
 * a clock; this is where it is made with time to think, and where one already
 * made can be found and changed. Without it a preference could only ever be
 * set during the thirty seconds a reader is also banning and trading, and a
 * favourite could only be marked in the place it exists to save them from.
 *
 * Only champions with mods. The roster is two hundred and forty names and the
 * rest of them have nothing to configure.
 */
export function ChampionsBrowser() {
  const champions = useQuery(champSelectQueries.withMods());
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  if (champions.isPending) return <Spinner className="m-auto" />;

  const all = champions.data ?? [];
  if (all.length === 0) {
    return (
      <EmptyState title={m.champions_empty_title()} description={m.champions_empty_description()} />
    );
  }

  const needle = query.trim().toLowerCase();
  const shown = all.filter((entry) => entry.champion.name.toLowerCase().includes(needle));
  const open = all.find((entry) => entry.champion.alias === selected);

  return (
    <div data-ui="ChampionsBrowser" className="flex h-full min-h-0">
      <div className="flex w-64 shrink-0 flex-col border-r border-surface-700/60">
        <div className="relative flex items-center p-2">
          <MagnifyingGlassIcon className="pointer-events-none absolute left-5 h-4 w-4 text-surface-500" />
          <Field.Control
            type="text"
            placeholder={m.champions_search_placeholder()}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="pl-9"
          />
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-2 pt-0 select-none">
          {shown.map((entry) => (
            <ChampionButton
              key={entry.champion.alias}
              entry={entry}
              selected={entry.champion.alias === selected}
              onSelect={() => setSelected(entry.champion.alias)}
            />
          ))}
          {shown.length === 0 && (
            <p className="px-2 py-4 text-meta text-surface-400">{m.champions_no_match_empty()}</p>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {!open && (
          <p className="text-meta text-surface-400 select-none">{m.champions_pick_one_hint()}</p>
        )}
        {open && (
          <div className="flex max-w-2xl flex-col gap-3">
            <h2 className="text-lg font-medium text-surface-100 select-none">
              {open.champion.name}
            </h2>
            <ChampionModList alias={open.champion.alias} name={open.champion.name} />
          </div>
        )}
      </div>
    </div>
  );
}

interface ChampionButtonProps {
  entry: ChampionMods;
  selected: boolean;
  onSelect: () => void;
}

function ChampionButton({ entry, selected, onSelect }: ChampionButtonProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={twMerge(
        "flex w-full items-baseline justify-between gap-2 rounded-md px-2 py-1.5 text-left",
        "hover:bg-surface-700/60",
        selected && "bg-accent-500/12 text-accent-300",
      )}
    >
      <span className="truncate text-row">{entry.champion.name}</span>
      <span className="shrink-0 text-fine text-surface-400">{entry.modIds.length}</span>
    </button>
  );
}

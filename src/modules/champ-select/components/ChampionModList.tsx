import { useQuery } from "@tanstack/react-query";

import { EmptyState, Spinner, useToast } from "@/components";
import { errorSummary, m } from "@/i18n";
import type { InstalledMod } from "@/lib/tauri";
import { ModThumbnails, useInstalledMods, useModThumbnail } from "@/modules/library";

import { champSelectQueries, useSetChampionFavorite, useSetChampionPreference } from "../api";
import { offerOrder } from "../utils/order";
import { ChampionModRow } from "./ChampionModRow";

interface ChampionModListProps {
  /** The champion's alias, which is what the library joins mods on. */
  alias: string;
  /** The champion's name, for the copy a reader reads. */
  name: string;
}

/**
 * Every mod a champion can apply, and which one it does.
 *
 * The same list in champion select and on the champions page, from one
 * component, because the two must not drift: a reader who curates a champion
 * calmly and then meets it under a clock has to recognise what they set up.
 *
 * The clock is the only difference between the two, and it belongs to the panel
 * around this rather than to the rows.
 */
export function ChampionModList({ alias, name }: ChampionModListProps) {
  const toast = useToast();
  const mods = useQuery(champSelectQueries.modsFor(alias));
  const preferences = useQuery(champSelectQueries.preferences());
  const favorites = useQuery(champSelectQueries.favorites());
  const installed = useInstalledMods();
  const setPreference = useSetChampionPreference();
  const setFavorite = useSetChampionFavorite();

  /* An entry that is not there and an entry whose mod is `null` are different:
     silence, and the reader having chosen no mod. Only the second is a choice,
     and only the second draws a tick. */
  const preference = preferences.data?.[alias];
  const preferred = preference?.preferred ?? null;
  const chosen = preference !== undefined;
  const marked = favorites.data?.[alias] ?? [];
  const modIds = offerOrder(mods.data ?? [], marked);

  const byId = new Map((installed.data ?? []).map((mod: InstalledMod) => [mod.id, mod]));

  const choose = (modId: string | null) => {
    setPreference.mutate(
      { champion: alias, modId },
      {
        onError: (error) => toast.error(m.champ_select_mods_failed_title(), errorSummary(error)),
      },
    );
  };

  if (mods.isPending) return <Spinner className="self-center" />;

  if (modIds.length === 0) {
    return (
      <EmptyState
        size="sm"
        title={m.champ_select_empty_title({ champion: name })}
        description={m.champ_select_empty_description()}
      />
    );
  }

  return (
    <ModThumbnails modIds={modIds}>
      <div data-ui="ChampionModList" className="flex flex-col gap-1">
        <ChampionModRow
          label={m.champ_select_no_mod_label()}
          detail={m.champ_select_no_mod_description()}
          selected={chosen && preferred === null}
          disabled={setPreference.isPending}
          onSelect={() => choose(null)}
        />
        {modIds.map((modId) => (
          <ModChoice
            key={modId}
            modId={modId}
            mod={byId.get(modId)}
            selected={preferred === modId}
            disabled={setPreference.isPending}
            onSelect={() => choose(modId)}
            favorite={marked.includes(modId)}
            onToggleFavorite={() =>
              setFavorite.mutate({
                champion: alias,
                modId,
                favorite: !marked.includes(modId),
              })
            }
          />
        ))}
      </div>
    </ModThumbnails>
  );
}

interface ModChoiceProps {
  modId: string;
  /** The library's entry, absent while the mods list has not answered yet. */
  mod: InstalledMod | undefined;
  selected: boolean;
  disabled: boolean;
  onSelect: () => void;
  favorite: boolean;
  onToggleFavorite: () => void;
}

function ModChoice({
  modId,
  mod,
  selected,
  disabled,
  onSelect,
  favorite,
  onToggleFavorite,
}: ModChoiceProps) {
  const thumbnail = useModThumbnail(modId);

  return (
    <ChampionModRow
      favorite={favorite}
      onToggleFavorite={onToggleFavorite}
      label={mod?.displayName ?? modId}
      detail={mod?.authors.join(", ") ?? ""}
      thumbnailUrl={thumbnail.data}
      selected={selected}
      disabled={disabled}
      onSelect={onSelect}
    />
  );
}

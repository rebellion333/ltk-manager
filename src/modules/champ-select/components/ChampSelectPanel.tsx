import { useQuery } from "@tanstack/react-query";

import { Button, Dialog, EmptyState, Spinner, useToast } from "@/components";
import { errorSummary, m } from "@/i18n";
import type { ChampionSummary, ChampSelectView, InstalledMod } from "@/lib/tauri";
import { ModThumbnails, useInstalledMods, useModThumbnail } from "@/modules/library";
import { usePatcherRunning } from "@/modules/patcher";
import { useQueuedDialog } from "@/stores";

import { champSelectQueries, useSetChampionFavorite, useSetChampionPreference } from "../api";
import { championInPlay, useChampSelectStore } from "../state/champSelect";
import { offerOrder } from "../utils/order";
import { formatSpan } from "../utils/span";
import { ChampionModRow } from "./ChampionModRow";
import { SwapStatus } from "./SwapStatus";

/**
 * The mods for the champion being picked, raised while champion select runs.
 *
 * It waits for a champion rather than for the select: a panel with nothing in
 * it yet is a panel the reader dismisses before it has anything to say. The
 * first hover is the moment it has something.
 *
 * Queued per ADR-0022, and first in that order because it is the only dialog
 * with a deadline.
 */
export function ChampSelectPanel() {
  const view = useChampSelectStore((state) => state.view);
  const dismissedFor = useChampSelectStore((state) => state.dismissedFor);
  const championId = championInPlay(view);
  const dismissed = championId !== null && championId === dismissedFor;
  const roster = useQuery(champSelectQueries.roster(championId !== null));
  const champion = roster.data?.find((entry) => entry.id === championId);

  /* The roster first, because a champion it has not answered for yet is
     indistinguishable from one it cannot name. Raising before it answers
     flashes "unrecognised" across every champion select. */
  const ready = championId !== null && !roster.isPending;
  const showing = useQueuedDialog("champ-select", ready && !dismissed);

  if (!view || championId === null || !showing) return null;

  return <ChampSelectDialog view={view} championId={championId} champion={champion} />;
}

interface ChampSelectDialogProps {
  view: ChampSelectView;
  championId: number;
  /** The roster's entry, absent when it does not carry this id. */
  champion: ChampionSummary | undefined;
}

function ChampSelectDialog({ view, championId, champion }: ChampSelectDialogProps) {
  const dismiss = useChampSelectStore((state) => state.dismiss);

  const locked = view.lockedChampionId !== null;
  const title = champion?.name ?? m.champ_select_unknown_champion_title();

  return (
    <Dialog.Shell
      open
      onClose={dismiss}
      title={title}
      description={m.champ_select_description()}
      size="md"
    >
      {!champion && (
        <Dialog.Body>
          <p className="text-sm text-surface-300">
            {m.champ_select_unknown_champion_description({ id: championId })}
          </p>
        </Dialog.Body>
      )}
      {champion && <ChampionMods champion={champion} locked={locked} />}

      <Dialog.Footer>
        <Button variant="ghost" onClick={dismiss}>
          {m.champ_select_close_action()}
        </Button>
      </Dialog.Footer>
    </Dialog.Shell>
  );
}

interface ChampionModsProps {
  champion: ChampionSummary;
  /** Whether the pick is locked, which is only shown, never acted on. */
  locked: boolean;
}

function ChampionMods({ champion, locked }: ChampionModsProps) {
  const toast = useToast();
  const mods = useQuery(champSelectQueries.modsFor(champion.alias));
  const preferences = useQuery(champSelectQueries.preferences());
  const favorites = useQuery(champSelectQueries.favorites());
  const status = useQuery(champSelectQueries.status());
  const installed = useInstalledMods();
  const setPreference = useSetChampionPreference();
  const setFavorite = useSetChampionFavorite();
  const lastReport = useChampSelectStore((state) => state.lastReport);
  const patcherRunning = usePatcherRunning();

  /* An entry that is not there and an entry whose mod is `null` are different:
     silence, and the reader having chosen no mod. Only the second is a choice,
     and only the second draws a tick. */
  const preference = preferences.data?.[champion.alias];
  const preferred = preference?.preferred ?? null;
  const chosen = preference !== undefined;
  const marked = favorites.data?.[champion.alias] ?? [];
  const modIds = offerOrder(mods.data ?? [], marked);

  const byId = new Map((installed.data ?? []).map((mod: InstalledMod) => [mod.id, mod]));

  const choose = (modId: string | null) => {
    setPreference.mutate(
      { champion: champion.alias, modId },
      {
        onError: (error) => toast.error(m.champ_select_mods_failed_title(), errorSummary(error)),
      },
    );
  };

  return (
    <Dialog.Body className="flex flex-col gap-3">
      <p className="text-fine text-surface-400 select-none">
        {locked && m.champ_select_locked_label()}
        {!locked && m.champ_select_hovering_label()}
      </p>

      {mods.isPending && <Spinner className="self-center" />}

      {!mods.isPending && modIds.length === 0 && (
        <EmptyState
          size="sm"
          title={m.champ_select_empty_title({ champion: champion.name })}
          description={m.champ_select_empty_description()}
        />
      )}

      {modIds.length > 0 && (
        <ModThumbnails modIds={modIds}>
          <div data-ui="ChampSelectPanel:choices" className="flex flex-col gap-1">
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
                    champion: champion.alias,
                    modId,
                    favorite: !marked.includes(modId),
                  })
                }
              />
            ))}
          </div>
        </ModThumbnails>
      )}

      {/* The scheduler reports a refusal it can never take back, and stays quiet
          about one that a later tick could turn into a swap. An idle patcher is
          the quiet one that lasts, so the panel says it rather than answering a
          click with nothing. */}
      {!patcherRunning && (
        <p className="text-meta text-surface-300 select-none">
          {m.champ_select_status_refused_patcher_idle()}
        </p>
      )}
      {patcherRunning && <SwapStatus report={lastReport} pending={setPreference.isPending} />}

      {patcherRunning && status.data && (
        <p className="text-fine text-surface-400 select-none">
          {m.champ_select_budget_hint({
            rebuild: formatSpan(status.data.rebuildMs),
            tail: formatSpan(status.data.tailMs),
          })}
        </p>
      )}
    </Dialog.Body>
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

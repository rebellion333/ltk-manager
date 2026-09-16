import { useQuery } from "@tanstack/react-query";

import { Button, Dialog } from "@/components";
import { m } from "@/i18n";
import type { ChampionSummary, ChampSelectView } from "@/lib/tauri";
import { usePatcherRunning } from "@/modules/patcher";
import { useQueuedDialog } from "@/stores";

import { champSelectQueries } from "../api";
import { championInPlay, useChampSelectStore } from "../state/champSelect";
import { formatSpan } from "../utils/span";
import { ChampionBench } from "./ChampionBench";
import { ChampionModList } from "./ChampionModList";
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
      {champion && <ChampionMods champion={champion} locked={locked} view={view} />}

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
  view: ChampSelectView;
}

function ChampionMods({ champion, locked, view }: ChampionModsProps) {
  const status = useQuery(champSelectQueries.status());
  const lastReport = useChampSelectStore((state) => state.lastReport);
  const patcherRunning = usePatcherRunning();

  return (
    <Dialog.Body className="flex flex-col gap-3">
      <p className="text-fine text-surface-400 select-none">
        {locked && m.champ_select_locked_label()}
        {!locked && m.champ_select_hovering_label()}
      </p>

      <ChampionModList alias={champion.alias} name={champion.name} />

      <ChampionBench championIds={view.benchChampionIds} open={view.canStillChange} />

      {/* The scheduler reports a refusal it can never take back, and stays quiet
          about one that a later tick could turn into a swap. An idle patcher is
          the quiet one that lasts, so the panel says it rather than answering a
          click with nothing. */}
      {!patcherRunning && (
        <p className="text-meta text-surface-300 select-none">
          {m.champ_select_status_refused_patcher_idle()}
        </p>
      )}
      {patcherRunning && <SwapStatus report={lastReport} />}

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

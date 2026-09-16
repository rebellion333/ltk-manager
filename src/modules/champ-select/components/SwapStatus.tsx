import { CheckCircleIcon, SpinnerGapIcon, WarningCircleIcon } from "@phosphor-icons/react";
import { match } from "ts-pattern";

import { m } from "@/i18n";
import type { Refusal, SwapReport } from "@/lib/tauri";

import { formatSpan } from "../utils/span";

/** What a refusal says, by its code and whatever fields that code carries. */
function refusalText(why: Refusal): string {
  return match(why)
    .with({ code: "patcherIdle" }, () => m.champ_select_status_refused_patcher_idle())
    .with({ code: "patcherBuilding" }, () => m.champ_select_status_refused_patcher_building())
    .with({ code: "gameAlreadyRunning" }, () =>
      m.champ_select_status_refused_game_already_running(),
    )
    .with({ code: "gameStarting" }, () => m.champ_select_status_refused_game_starting())
    .with({ code: "champSelectOver" }, () => m.champ_select_status_refused_champ_select_over())
    .with({ code: "buildInFlight" }, () => m.champ_select_status_refused_build_in_flight())
    .with({ code: "notEnoughTime" }, ({ neededMs, availableMs }) =>
      m.champ_select_status_refused_not_enough_time({
        needed: formatSpan(neededMs),
        available: formatSpan(availableMs),
      }),
    )
    .exhaustive();
}

interface SwapStatusProps {
  /** What the scheduler last said, or `null` while it has said nothing. */
  report: SwapReport | null;
  /** Whether a swap the reader asked for by hand is still in flight. */
  pending: boolean;
}

/**
 * The one line saying what the overlay is actually carrying.
 *
 * It reports rather than reassures: a refusal says the swap did not happen and
 * what the reader keeps instead, because a panel that only ever shows a tick is
 * a panel that lies the first time a rebuild does not fit.
 */
export function SwapStatus({ report, pending }: SwapStatusProps) {
  if (pending) {
    return (
      <p
        data-ui="SwapStatus"
        className="flex items-center gap-2 text-meta text-surface-300 select-none"
      >
        <SpinnerGapIcon className="h-4 w-4 shrink-0 animate-spin" />
        {m.champ_select_working_status()}
      </p>
    );
  }

  if (!report) return null;

  const applied = report.status === "applied";
  const text = match(report)
    .with({ status: "applied" }, ({ modId, tookMs }) => {
      const took = formatSpan(tookMs);
      if (modId) return m.champ_select_status_applied({ took });
      return m.champ_select_status_cleared({ took });
    })
    .with({ status: "refused" }, ({ why }) => refusalText(why))
    .with({ status: "failed" }, () => m.champ_select_status_failed())
    .exhaustive();

  return (
    <p data-ui="SwapStatus" className="flex items-start gap-2 text-meta text-surface-300">
      {applied && <CheckCircleIcon className="mt-0.5 h-4 w-4 shrink-0 text-success-text" />}
      {!applied && <WarningCircleIcon className="mt-0.5 h-4 w-4 shrink-0 text-warning-text" />}
      <span className="select-text">{text}</span>
    </p>
  );
}

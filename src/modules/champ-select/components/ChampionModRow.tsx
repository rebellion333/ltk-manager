import { CheckIcon } from "@phosphor-icons/react";

import { MaskIcon } from "@/components";
import { twMerge } from "@/utils";

interface ChampionModRowProps {
  /** What the mod is called, or the copy for the row that applies none. */
  label: string;
  /** The line under the label: the mod's authors, or what the row does. */
  detail: string;
  thumbnailUrl?: string;
  selected: boolean;
  disabled: boolean;
  onSelect: () => void;
}

/**
 * One choice for the champion in play, applied by clicking it.
 *
 * A row rather than a card: the reader is reading this while a champion select
 * clock runs, so the list has to be scannable at a glance and one click deep.
 */
export function ChampionModRow({
  label,
  detail,
  thumbnailUrl,
  selected,
  disabled,
  onSelect,
}: ChampionModRowProps) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onSelect}
      className={twMerge(
        "flex w-full items-center gap-3 rounded-lg border border-transparent px-2.5 py-2 text-left select-none",
        "hover:bg-surface-700/60 disabled:pointer-events-none disabled:opacity-50",
        selected && "border-accent-500/40 bg-accent-500/12",
      )}
    >
      <span className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-md bg-surface-700">
        {thumbnailUrl && <img src={thumbnailUrl} alt="" className="h-full w-full object-cover" />}
        {!thumbnailUrl && <MaskIcon className="h-4 w-4 text-surface-400" />}
      </span>

      <span className="min-w-0 flex-1">
        <span className="block truncate text-row text-surface-100">{label}</span>
        <span className="block truncate text-fine text-surface-400">{detail}</span>
      </span>

      {selected && <CheckIcon className="h-4 w-4 shrink-0 text-accent-300" weight="bold" />}
    </button>
  );
}

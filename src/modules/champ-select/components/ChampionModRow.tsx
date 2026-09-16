import { CheckIcon, StarIcon } from "@phosphor-icons/react";

import { MaskIcon } from "@/components";
import { m } from "@/i18n";
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
  /** Whether the row offers a favourite star. The "no mod" row does not. */
  favorite?: boolean;
  onToggleFavorite?: () => void;
}

/**
 * One choice for the champion in play, applied by clicking it.
 *
 * A row rather than a card: the reader is reading this while a champion select
 * clock runs, so the list has to be scannable at a glance and one click deep.
 *
 * The star is its own button beside the row rather than inside it, because a
 * button inside a button is not a thing, and because marking a favourite and
 * applying a mod are different enough that one must not be a mis-click of the
 * other.
 */
export function ChampionModRow({
  label,
  detail,
  thumbnailUrl,
  selected,
  disabled,
  onSelect,
  favorite,
  onToggleFavorite,
}: ChampionModRowProps) {
  return (
    <div
      className={twMerge(
        "flex items-center gap-1 rounded-lg border border-transparent pr-1 select-none",
        "hover:bg-surface-700/60",
        selected && "border-accent-500/40 bg-accent-500/12",
      )}
    >
      <button
        type="button"
        disabled={disabled}
        onClick={onSelect}
        className="flex min-w-0 flex-1 items-center gap-3 px-2.5 py-2 text-left disabled:pointer-events-none disabled:opacity-50"
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

      {onToggleFavorite && (
        <FavoriteStar favorite={favorite === true} onToggle={onToggleFavorite} />
      )}
    </div>
  );
}

interface FavoriteStarProps {
  favorite: boolean;
  onToggle: () => void;
}

/** The star that keeps a mod at the top of this champion's list. */
function FavoriteStar({ favorite, onToggle }: FavoriteStarProps) {
  const label = favorite ? m.champ_select_unfavorite_action() : m.champ_select_favorite_action();
  const weight = favorite ? "fill" : "regular";

  return (
    <button
      type="button"
      onClick={onToggle}
      aria-pressed={favorite}
      aria-label={label}
      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-surface-400 hover:bg-surface-600/60 hover:text-surface-200"
    >
      <StarIcon className={twMerge("h-4 w-4", favorite && "text-warning-text")} weight={weight} />
    </button>
  );
}

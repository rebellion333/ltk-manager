/**
 * The champion's mods in the order champion select offers them.
 *
 * Favourites first, in the order the reader marked them, then everything else
 * in library order. The reader's order is the one that survives: a list sorted
 * by anything else would move the row they were reaching for between one
 * champion select and the next.
 *
 * A favourite the champion no longer has a mod for is dropped rather than left
 * as a gap, which is what makes this safe against a list and a favourites map
 * that were read a moment apart.
 */
export function offerOrder(modIds: readonly string[], favorites: readonly string[]): string[] {
  const available = new Set(modIds);
  const first = favorites.filter((id) => available.has(id));
  const marked = new Set(first);
  return [...first, ...modIds.filter((id) => !marked.has(id))];
}

/** SwiftUI destinations count positions before the moved source rows are removed. */
export function moveHomeOrder(
  ids: readonly string[],
  sources: readonly number[],
  destination: number,
): string[] | null {
  if (
    !Number.isSafeInteger(destination) ||
    destination < 0 ||
    destination > ids.length ||
    sources.length === 0 ||
    new Set(ids).size !== ids.length ||
    new Set(sources).size !== sources.length ||
    sources.some((index) => !Number.isSafeInteger(index) || index < 0 || index >= ids.length)
  )
    return null;
  const selected = new Set(sources);
  const moved = ids.filter((_, index) => selected.has(index));
  const remaining = ids.filter((_, index) => !selected.has(index));
  const insertion = destination - sources.filter((index) => index < destination).length;
  remaining.splice(insertion, 0, ...moved);
  return remaining;
}

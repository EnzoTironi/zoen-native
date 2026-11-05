import type { SpaceSummary } from './generated/SpaceSummary';
import type { UniversalHit } from './generated/UniversalHit';
import type { Persona } from './generated/Persona';

export type SearchIdentityOption = Readonly<{ id: string; title: string; person?: Persona }>;
/** Presentation candidates only. Core revalidates access and applies exact identity filters. */
export function searchIdentityOptions(
  spaces: readonly SpaceSummary[],
  hits: readonly UniversalHit[],
  spaceId?: string,
) {
  const authors = new Map<string, SearchIdentityOption>();
  for (const space of spaces) {
    if (spaceId && space.id !== spaceId) continue;
    for (const person of space.members)
      authors.set(person.id, { id: person.id, title: person.name, person });
  }
  for (const hit of hits) {
    if (hit.kind !== 'message' || !hit.persona || (spaceId && hit.space_id !== spaceId)) continue;
    const person = hit.persona;
    authors.set(person.id, { id: person.id, title: person.name, person });
  }
  const order = (a: SearchIdentityOption, b: SearchIdentityOption) =>
    a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
  return {
    spaces: spaces.map((space) => ({ id: space.id, title: space.title })).sort(order),
    authors: [...authors.values()].sort(order),
  };
}

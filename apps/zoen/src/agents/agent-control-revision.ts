/** The relay's signed SQL revision, kept exact across the JSON bridge. */
export const agentControlRevisionMax = 9_223_372_036_854_775_807n;
export function agentControlRevision(value: unknown): bigint | null {
  if (typeof value !== 'string' || value.length > 19 || !/^[1-9][0-9]*$/.test(value)) return null;
  const revision = BigInt(value);
  return revision <= agentControlRevisionMax ? revision : null;
}

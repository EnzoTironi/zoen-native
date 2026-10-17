import { hostname } from './sandbox.ts';

/** Grant-list projections are not authority: each exact host is checked for the
 * current owner by Rust before it can enter this frame's CSP. */
export async function authorizedNetworkHosts(
  hosts: readonly string[],
  allowed: (capability: string) => Promise<boolean>,
  assertActive: () => void,
): Promise<string[]> {
  if (!hosts.every(hostname)) throw new Error('Domínio de miniapp inválido.');
  assertActive();
  const decisions = await Promise.all(hosts.map((host) => allowed(`net:${host}`)));
  assertActive();
  return hosts.filter((_, index) => decisions[index]);
}

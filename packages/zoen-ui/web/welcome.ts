import { inviteFromPath } from './session.ts';
export function completeWelcome() { localStorage.setItem('zoen.welcomed', 'true'); }
export function openWelcomeInvite(value: string): boolean {
  try { const path = new URL(value, location.origin).pathname; if (!inviteFromPath(path)) return false; completeWelcome(); location.assign(path); return true; }
  catch { return false; }
}

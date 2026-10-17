import type { SurfacePort } from './coordinator';

export function surfacesAvailable(): boolean {
  return false;
}
export async function createSurfacePort(): Promise<SurfacePort | null> {
  return null;
}

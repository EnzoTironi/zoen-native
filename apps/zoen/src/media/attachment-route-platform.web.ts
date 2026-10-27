import type { StaticLocation } from './types';
import type { SharedContact } from './attachment-content';
export async function chooseSharedContact(_current: () => boolean): Promise<SharedContact | null> {
  throw new Error('contact-unavailable');
}
export async function chooseStaticPoint(_current: () => boolean): Promise<StaticLocation | null> {
  throw new Error('location-unavailable');
}
export async function prepareContactFile(
  _text: string,
): Promise<{ uri: string; release(): Promise<void> }> {
  throw new Error('contact-unavailable');
}

export const prepareLocationFile = prepareContactFile;

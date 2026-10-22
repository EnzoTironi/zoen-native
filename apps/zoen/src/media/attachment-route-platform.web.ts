import type { SharedContact } from './attachment-content';
export async function chooseSharedContact(_current: () => boolean): Promise<SharedContact | null> {
  throw new Error('contact-unavailable');
}
export async function chooseStaticLocation(_current: () => boolean): Promise<string | null> {
  throw new Error('location-unavailable');
}
export async function prepareContactFile(
  _text: string,
): Promise<{ uri: string; release(): Promise<void> }> {
  throw new Error('contact-unavailable');
}

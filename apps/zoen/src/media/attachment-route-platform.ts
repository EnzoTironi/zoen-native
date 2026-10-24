import { Contact, ContactField } from 'expo-contacts';
import * as Location from 'expo-location';
import { Directory, File, Paths } from 'expo-file-system';
import { randomUUID } from 'expo-crypto';
import type { StaticLocation } from './types';
import { sharedContact, type SharedContact } from './attachment-content';

export async function chooseSharedContact(current: () => boolean): Promise<SharedContact | null> {
  const contact = await Contact.presentPicker();
  if (!contact || !current()) return null;
  const details = await contact.getDetails([ContactField.FULL_NAME, ContactField.PHONES] as const);
  if (!current()) return null;
  return sharedContact(
    details.fullName ?? '',
    (details.phones ?? [])
      .map((phone) => phone.number)
      .filter((phone): phone is string => typeof phone === 'string' && phone.length > 0),
  );
}
export async function chooseStaticPoint(current: () => boolean): Promise<StaticLocation | null> {
  const permission = await Location.requestForegroundPermissionsAsync();
  if (!current()) return null;
  if (!permission.granted) throw new Error('location-permission');
  const location = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
  return current()
    ? { latitude: location.coords.latitude, longitude: location.coords.longitude }
    : null;
}
/** The existing media channel retains this cleanup until canonical ingress has reconciled. */
export async function prepareReviewedFile(
  text: string,
  name: 'contact.vcf' | 'location.geojson',
): Promise<{ uri: string; release(): Promise<void> }> {
  const directory = new Directory(Paths.cache, 'zoen-reviewed-attachments', randomUUID());
  directory.create({ intermediates: true });
  const file = new File(directory, name);
  try {
    file.create();
    await file.write(text);
  } catch (error) {
    if (directory.exists) directory.delete();
    throw error;
  }
  return {
    uri: file.uri,
    release: async () => {
      if (directory.exists) directory.delete();
    },
  };
}

export const prepareContactFile = (text: string) => prepareReviewedFile(text, 'contact.vcf');
export const prepareLocationFile = (text: string) => prepareReviewedFile(text, 'location.geojson');

import { requireOptionalNativeModule } from 'expo';
import { AppState, Platform } from 'react-native';
import * as Device from 'expo-device';
import { File, Paths } from 'expo-file-system';
import {
  PhotoVault,
  MAX_PHOTO_SOURCE_BYTES,
  busyBlocks,
  eventDraft,
  photoOptions,
  pickedContact,
  place,
  timeWindow,
  type CapabilityLease,
  type PhotoPreview,
} from './capability-data';
import { jpegPreview } from './photo-preview';
import { sensitiveCacheWork } from '../media/cache-work';

const photos = new PhotoVault();
let presenting = false;
const has = (name: string) => requireOptionalNativeModule(name) !== null;

/** Only compiled, implemented adapters are advertised; an older dev build still opens. */
export function available(): string[] {
  if (Platform.OS !== 'ios') return [];
  const capabilities: string[] = [];
  if (has('ExponentImagePicker') && has('ExpoImageManipulator') && has('ExpoCrypto')) {
    capabilities.push('photos.pick');
    if (Device.isDevice) capabilities.push('camera.capture');
  }
  if (has('ExpoLocation')) capabilities.push('location', 'location.approximate');
  if (has('ExpoCalendar')) capabilities.push('calendar.freebusy', 'calendar.add');
  if (has('ExpoContacts')) capabilities.push('contacts.pick');
  return capabilities;
}

function releaseCache(uri: string): void {
  // Every URI here came from our picker/manipulator, never from miniapp parameters.
  if (!uri.startsWith(Paths.cache.uri)) return;
  try {
    const file = new File(uri);
    if (file.exists) file.delete();
  } catch {
    /* OS cache eviction. */
  }
}

async function pickPhoto(
  params: Record<string, unknown>,
  lease: CapabilityLease,
  camera: boolean,
): Promise<PhotoPreview[]> {
  const Picker = await import('expo-image-picker');
  const { randomUUID } = await import('expo-crypto');
  const options = photoOptions(params);
  lease.check();
  if (camera && !(await Picker.requestCameraPermissionsAsync()).granted)
    throw new Error('Permita a câmera nos Ajustes para tirar uma foto.');
  lease.check();
  const result = camera
    ? await Picker.launchCameraAsync({
        mediaTypes: ['images'],
        exif: false,
        base64: false,
        quality: 1,
        cameraType: Picker.CameraType.back,
      })
    : await Picker.launchImageLibraryAsync({
        mediaTypes: ['images'],
        allowsMultipleSelection: options.max > 1,
        selectionLimit: options.max,
        exif: false,
        base64: false,
        quality: 1,
      });
  if (result.canceled) return [];
  try {
    lease.check();
    const previews: Omit<PhotoPreview, 'token'>[] = [];
    for (const asset of result.assets.slice(0, options.max)) {
      const size = asset.fileSize ?? new File(asset.uri).size;
      if (asset.type !== 'image' || size < 1 || size > MAX_PHOTO_SOURCE_BYTES)
        throw new Error('Escolha uma foto de até 32 MB.');
      const preview = await jpegPreview(asset.uri, options.maxSide, lease, releaseCache);
      previews.push(preview);
    }
    return previews.map((preview) => photos.put(lease, randomUUID(), preview));
  } finally {
    for (const asset of result.assets) releaseCache(asset.uri);
  }
}

async function location(approximate: boolean, lease: CapabilityLease) {
  const Location = await import('expo-location');
  lease.check();
  if (!(await Location.requestForegroundPermissionsAsync()).granted)
    throw new Error('Permita a localização durante o uso nos Ajustes.');
  lease.check();
  return new Promise<ReturnType<typeof place>>((resolve, reject) => {
    let watcher: { remove(): void } | undefined;
    let settled = false;
    const finish = (result?: ReturnType<typeof place>, failure?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      lease.signal.removeEventListener('abort', cancel);
      watcher?.remove();
      if (failure) reject(failure);
      else resolve(result!);
    };
    const cancel = () => finish(undefined, new Error('Este miniapp foi fechado.'));
    const timer = setTimeout(
      () => finish(undefined, new Error('A localização demorou demais. Tente novamente.')),
      12_000,
    );
    lease.signal.addEventListener('abort', cancel, { once: true });
    void Location.watchPositionAsync(
      {
        accuracy: approximate ? Location.Accuracy.Low : Location.Accuracy.Balanced,
        distanceInterval: 0,
      },
      (fix) => {
        try {
          lease.check();
          finish(
            place(fix.coords.latitude, fix.coords.longitude, fix.coords.accuracy, approximate),
          );
        } catch (failure) {
          finish(undefined, failure);
        }
      },
      () => finish(undefined, new Error('Não foi possível obter sua localização.')),
    ).then(
      (value) => {
        watcher = value;
        if (settled) watcher.remove();
      },
      (failure) => finish(undefined, failure),
    );
  });
}

function releaseShared(value: unknown): void {
  if (
    value &&
    typeof value === 'object' &&
    'release' in value &&
    typeof value.release === 'function'
  )
    value.release();
}

async function freebusy(params: Record<string, unknown>, lease: CapabilityLease) {
  const range = timeWindow(params);
  const Calendar = await import('expo-calendar');
  lease.check();
  if (!(await Calendar.requestCalendarPermissions(false)).granted)
    throw new Error('Permita a leitura do calendário nos Ajustes para consultar horários livres.');
  lease.check();
  const calendars = await Calendar.getCalendars(Calendar.EntityTypes.EVENT);
  let events: Awaited<ReturnType<typeof Calendar.listEvents>> = [];
  try {
    lease.check();
    events = await Calendar.listEvents(calendars, new Date(range.startMs), new Date(range.endMs));
    lease.check();
    return busyBlocks(
      events.map((event) => ({
        startMs: new Date(event.startDate).getTime(),
        endMs: new Date(event.endDate).getTime(),
        allDay: event.allDay,
        free: event.availability === Calendar.Availability.FREE,
      })),
      range,
    );
  } finally {
    events.forEach(releaseShared);
    calendars.forEach(releaseShared);
  }
}

async function addEvent(params: Record<string, unknown>, lease: CapabilityLease) {
  const draft = eventDraft(params);
  const Calendar = await import('expo-calendar');
  lease.check();
  if (!(await Calendar.requestCalendarPermissions(true)).granted)
    throw new Error('Permita adicionar eventos ao calendário nos Ajustes.');
  lease.check();
  const calendar = Calendar.getDefaultCalendarSync();
  try {
    const result = await calendar.addEventWithForm(draft);
    lease.check();
    return { saved: result.action === Calendar.CalendarDialogResultActions.saved };
  } finally {
    releaseShared(calendar);
  }
}

async function contact(lease: CapabilityLease) {
  // SDK58 Next resolves a selected ID through CNContactStore, which would need
  // broader permission. The explicit compatibility picker returns the chosen
  // CNContact directly; we expose its name only and never request the address book.
  const Contacts = await import('expo-contacts/legacy');
  lease.check();
  const chosen = await Contacts.presentContactPickerAsync();
  lease.check();
  return chosen
    ? pickedContact(chosen.name || [chosen.firstName, chosen.lastName].filter(Boolean).join(' '))
    : [];
}

export async function perform(
  cap: string,
  params: Record<string, unknown>,
  item: string,
): Promise<unknown> {
  if (!available().includes(cap))
    throw new Error('Este recurso não está disponível neste aparelho.');
  if (AppState.currentState !== 'active') throw new Error('Abra o Zoen para continuar.');
  if (presenting) throw new Error('Conclua o seletor aberto antes de começar outra ação.');
  const lease = photos.begin(item);
  presenting = true;
  const background = AppState.addEventListener('change', (state) => {
    if (state === 'background') photos.release(item);
  });
  try {
    let result: unknown;
    switch (cap) {
      case 'photos.pick':
        result = await sensitiveCacheWork.run(() => pickPhoto(params, lease, false));
        break;
      case 'camera.capture': {
        const selection = await sensitiveCacheWork.run(() =>
          pickPhoto({ max: 1, maxSide: 1024 }, lease, true),
        );
        if (!selection[0]) throw new Error('Você não tirou uma foto.');
        result = selection[0];
        break;
      }
      case 'location':
      case 'location.approximate':
        result = await location(cap === 'location.approximate', lease);
        break;
      case 'calendar.freebusy':
        result = await freebusy(params, lease);
        break;
      case 'calendar.add':
        result = await addEvent(params, lease);
        break;
      case 'contacts.pick':
        result = await contact(lease);
        break;
      default:
        throw new Error('Este recurso não está disponível neste aparelho.');
    }
    lease.check();
    return result;
  } finally {
    background.remove();
    presenting = false;
  }
}

export function resolvePhoto(token: string, item: string): string | null {
  return photos.resolve(token, item);
}
export function releaseItemPhotos(item: string): void {
  photos.release(item);
}

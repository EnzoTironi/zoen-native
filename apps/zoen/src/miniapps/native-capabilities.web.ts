import { t } from '../i18n/index.ts';
import {
  PhotoVault,
  MAX_PHOTO_SOURCE_BYTES,
  photoOptions,
  pickedContact,
  place,
  type CapabilityLease,
  type PhotoPreview,
} from './capability-data';
import { jpegPreview } from './photo-preview';

type BrowserContacts = {
  select(fields: string[], options: { multiple: boolean }): Promise<{ name?: string[] }[]>;
};
const photos = new PhotoVault();
let presenting = false;
const browserContacts = () => (navigator as Navigator & { contacts?: BrowserContacts }).contacts;

export function available(): string[] {
  if (typeof window === 'undefined' || typeof document === 'undefined') return [];
  const caps: string[] = [];
  const random = typeof globalThis.crypto?.getRandomValues === 'function';
  if (random) caps.push('photos.pick');
  if (!window.isSecureContext) return caps;
  if (random && typeof navigator.mediaDevices?.getUserMedia === 'function')
    caps.push('camera.capture');
  if (navigator.geolocation) caps.push('location', 'location.approximate');
  if (browserContacts()?.select) caps.push('contacts.pick');
  return caps;
}

function nonce(): string {
  return [...crypto.getRandomValues(new Uint8Array(16))]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

type Prompt<T> = {
  body: HTMLElement;
  buttons: HTMLElement;
  active(): boolean;
  done(value: T): void;
  fail(reason: unknown): void;
  cleanup(fn: () => void): void;
};
/** Native browser dialog supplies focus trapping and a fresh user gesture for pickers. */
function prompt<T>(
  title: string,
  lease: CapabilityLease,
  build: (prompt: Prompt<T>) => void,
): Promise<T> {
  lease.check();
  return new Promise<T>((resolve, reject) => {
    const previous = document.activeElement;
    const dialog = document.createElement('dialog');
    dialog.setAttribute('aria-label', title);
    dialog.style.cssText =
      'font:17px system-ui;color:CanvasText;background:Canvas;color-scheme:light dark;max-width:520px;width:calc(100% - 48px);box-sizing:border-box;border:1px solid GrayText;border-radius:22px;padding:24px;';
    const heading = document.createElement('h2');
    heading.textContent = title;
    heading.style.cssText = 'margin:0 0 18px;font-size:23px;';
    const body = document.createElement('div');
    const buttons = document.createElement('div');
    buttons.style.cssText =
      'display:flex;flex-wrap:wrap;justify-content:flex-end;gap:12px;margin-top:20px;';
    dialog.append(heading, body, buttons);
    let settled = false;
    const cleanups: (() => void)[] = [];
    const finish = (value?: T, reason?: unknown) => {
      if (settled) return;
      settled = true;
      lease.signal.removeEventListener('abort', cancel);
      for (const cleanup of cleanups) cleanup();
      dialog.close();
      dialog.remove();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
      if (reason) reject(reason);
      else resolve(value!);
    };
    const cancel = () => finish(undefined, new Error(t('miniapps.youCanceledThisAction')));
    dialog.addEventListener('cancel', (event) => {
      event.preventDefault();
      cancel();
    });
    lease.signal.addEventListener('abort', cancel, { once: true });
    document.body.append(dialog);
    try {
      build({
        body,
        buttons,
        active: () => !settled,
        done: (value) => finish(value),
        fail: (reason) => finish(undefined, reason),
        cleanup: (fn) => cleanups.push(fn),
      });
      buttons.append(button(t('miniapps.cancel'), cancel));
      dialog.showModal();
    } catch (reason) {
      finish(undefined, reason);
    }
  });
}

function button(label: string, action: () => void): HTMLButtonElement {
  const element = document.createElement('button');
  element.type = 'button';
  element.textContent = label;
  element.style.cssText =
    'font:inherit;min-height:44px;padding:10px 18px;border:1px solid GrayText;border-radius:12px;color:ButtonText;background:ButtonFace;cursor:pointer;';
  element.addEventListener('click', action);
  return element;
}

async function selectedPhotos(params: Record<string, unknown>, lease: CapabilityLease) {
  const options = photoOptions(params);
  const files = await prompt<File[]>(t('miniapps.choosePhotos'), lease, (ui) => {
    const message = document.createElement('p');
    message.textContent = t('miniapps.photoPickerHelp', { count: options.max });
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/jpeg,image/png,image/webp,image/avif,image/gif,image/heic,image/heif';
    input.multiple = options.max > 1;
    input.hidden = true;
    input.addEventListener('change', () => ui.done(Array.from(input.files ?? [])));
    input.addEventListener('cancel', () => ui.done([]));
    ui.body.append(message, input);
    ui.buttons.append(button(t('miniapps.choosePhotos'), () => input.click()));
  });
  lease.check();
  if (files.length > options.max)
    throw new Error(t('miniapps.photoPickerMaximum', { count: options.max }));
  const previews: Omit<PhotoPreview, 'token'>[] = [];
  for (const file of files) {
    if (
      file.size < 1 ||
      file.size > MAX_PHOTO_SOURCE_BYTES ||
      !/^image\/(jpeg|png|webp|avif|gif|hei[cf])$/i.test(file.type)
    )
      throw new Error(t('miniapps.chooseAPhotoInASupportedFormatUp'));
    const uri = URL.createObjectURL(file);
    try {
      previews.push(await jpegPreview(uri, options.maxSide, lease, URL.revokeObjectURL));
    } finally {
      URL.revokeObjectURL(uri);
    }
  }
  lease.check();
  return previews.map((preview) => photos.put(lease, nonce(), preview));
}

async function camera(lease: CapabilityLease): Promise<PhotoPreview> {
  const blob = await prompt<Blob>(t('miniapps.takePhoto'), lease, (ui) => {
    const video = document.createElement('video');
    video.autoplay = true;
    video.muted = true;
    video.playsInline = true;
    video.setAttribute('aria-label', t('miniapps.cameraPreview'));
    video.style.cssText =
      'width:100%;max-height:55vh;object-fit:contain;border-radius:12px;background:#111;';
    const status = document.createElement('p');
    status.textContent = t('miniapps.enableTheCameraToFrameYourPhoto');
    status.setAttribute('role', 'status');
    let stream: MediaStream | undefined;
    ui.cleanup(() => {
      stream?.getTracks().forEach((track) => track.stop());
      video.srcObject = null;
    });
    const capture = button(t('miniapps.takePhoto'), () => {
      if (!video.videoWidth || !video.videoHeight) return;
      capture.disabled = true;
      const scale = Math.min(1, 1024 / Math.max(video.videoWidth, video.videoHeight));
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.round(video.videoWidth * scale));
      canvas.height = Math.max(1, Math.round(video.videoHeight * scale));
      const context = canvas.getContext('2d');
      if (!context) {
        ui.fail(new Error(t('miniapps.theCameraCouldNotBePrepared')));
        return;
      }
      context.drawImage(video, 0, 0, canvas.width, canvas.height);
      canvas.toBlob(
        (value) => {
          canvas.width = 0;
          canvas.height = 0;
          if (value) ui.done(value);
          else ui.fail(new Error(t('miniapps.thePhotoCouldNotBeTaken')));
        },
        'image/jpeg',
        0.85,
      );
    });
    capture.disabled = true;
    video.addEventListener('loadeddata', () => {
      capture.disabled = false;
    });
    const start = button(t('miniapps.enableCamera'), () => {
      start.disabled = true;
      void navigator.mediaDevices
        .getUserMedia({
          video: {
            facingMode: { ideal: 'environment' },
            width: { ideal: 1280 },
            height: { ideal: 1280 },
          },
          audio: false,
        })
        .then(async (value) => {
          if (!ui.active()) {
            value.getTracks().forEach((track) => track.stop());
            return;
          }
          stream = value;
          video.srcObject = value;
          await video.play();
          status.textContent = t('miniapps.frameYourPhotoAndSelectTakePhoto');
          start.hidden = true;
        })
        .catch(() => ui.fail(new Error(t('miniapps.theCameraCouldNotBeOpenedCheckYour'))));
    });
    ui.body.append(video, status);
    ui.buttons.append(start, capture);
  });
  lease.check();
  const uri = URL.createObjectURL(blob);
  try {
    return photos.put(lease, nonce(), await jpegPreview(uri, 1024, lease, URL.revokeObjectURL));
  } finally {
    URL.revokeObjectURL(uri);
  }
}

function location(approximate: boolean, lease: CapabilityLease) {
  return new Promise<ReturnType<typeof place>>((resolve, reject) => {
    let watcher: number | undefined;
    let settled = false;
    const finish = (value?: ReturnType<typeof place>, reason?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (watcher !== undefined) navigator.geolocation.clearWatch(watcher);
      lease.signal.removeEventListener('abort', cancel);
      if (reason) reject(reason);
      else resolve(value!);
    };
    const cancel = () => finish(undefined, new Error(t('miniapps.thisMiniappHasClosed')));
    const timer = setTimeout(
      () => finish(undefined, new Error(t('miniapps.locationTookTooLongTryAgain'))),
      12_000,
    );
    lease.signal.addEventListener('abort', cancel, { once: true });
    watcher = navigator.geolocation.watchPosition(
      (fix) => {
        try {
          lease.check();
          finish(
            place(fix.coords.latitude, fix.coords.longitude, fix.coords.accuracy, approximate),
          );
        } catch (reason) {
          finish(undefined, reason);
        }
      },
      () => finish(undefined, new Error(t('miniapps.yourLocationCouldNotBeObtainedCheckYour'))),
      { enableHighAccuracy: !approximate, maximumAge: 0, timeout: 12_000 },
    );
    if (settled) navigator.geolocation.clearWatch(watcher);
  });
}

async function contact(lease: CapabilityLease) {
  return prompt<ReturnType<typeof pickedContact>>(t('miniapps.chooseContact'), lease, (ui) => {
    const message = document.createElement('p');
    message.textContent = t('miniapps.onlyTheChosenContactSNameWillBe');
    const choose = button(t('miniapps.chooseContact'), () => {
      choose.disabled = true;
      void browserContacts()!
        .select(['name'], { multiple: false })
        .then((chosen) => {
          lease.check();
          ui.done(pickedContact(chosen[0]?.name?.[0] ?? ''));
        })
        .catch(() => ui.fail(new Error(t('miniapps.youDidNotChooseAContact'))));
    });
    ui.body.append(message);
    ui.buttons.append(choose);
  });
}

export async function perform(
  cap: string,
  params: Record<string, unknown>,
  item: string,
): Promise<unknown> {
  if (!available().includes(cap))
    throw new Error(t('miniapps.thisFeatureIsNotAvailableInThisBrowser'));
  if (document.visibilityState !== 'visible') throw new Error(t('miniapps.openZoenToContinue'));
  if (presenting) throw new Error(t('miniapps.finishTheOpenPickerBeforeStartingAnotherAction'));
  const lease = photos.begin(item);
  presenting = true;
  const background = () => {
    if (document.visibilityState !== 'visible') photos.release(item);
  };
  document.addEventListener('visibilitychange', background);
  try {
    let result: unknown;
    switch (cap) {
      case 'photos.pick':
        result = await selectedPhotos(params, lease);
        break;
      case 'camera.capture':
        result = await camera(lease);
        break;
      case 'location':
      case 'location.approximate':
        result = await location(cap === 'location.approximate', lease);
        break;
      case 'contacts.pick':
        result = await contact(lease);
        break;
      default:
        throw new Error(t('miniapps.thisFeatureIsNotAvailableInThisBrowser'));
    }
    lease.check();
    return result;
  } finally {
    document.removeEventListener('visibilitychange', background);
    presenting = false;
  }
}

export function resolvePhoto(token: string, item: string): string | null {
  return photos.resolve(token, item);
}
export function releaseItemPhotos(item: string): void {
  photos.release(item);
}

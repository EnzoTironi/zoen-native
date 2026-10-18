import { t } from '../i18n/index.ts';
import { requireOptionalNativeModule } from 'expo';
import { File, Paths } from 'expo-file-system';
import * as AppIntents from 'expo-app-intents';
import { randomUUID } from 'expo-crypto';
import type { SurfacePort } from './coordinator';
import type { MiniappActivityContent } from './activity';
import { surfaceActivityURL } from './links';
import { publishWidgetCatalog } from './widget-publication';

export function surfacesAvailable(): boolean {
  return (
    !!requireOptionalNativeModule('AppIntentsSetup') &&
    !!requireOptionalNativeModule('ExpoWidgets') &&
    AppIntents.isAvailable()
  );
}

export async function createSurfacePort(): Promise<SurfacePort | null> {
  if (!surfacesAvailable()) return null;
  const [{ default: widget }, { default: activity }] = await Promise.all([
    import('./miniapp-widget.ios'),
    import('./miniapp-activity.ios'),
  ]);
  const file = new File(Paths.document, 'native-surfaces.json');
  function findActivity(id: string) {
    return activity.getInstances().find((value) => value.getId() === id);
  }
  return {
    async eraseMetadata() {
      // The app dispatches intents but never donates them. Deleting by an
      // unregistered donation name fails even when the device has no donations.
      await AppIntents.clearPendingInvocationsAsync();
    },
    async readPreferences() {
      if (!file.exists || file.size > 8_192) return null;
      return file.text();
    },
    async writePreferences(value) {
      if (value === null) {
        if (file.exists) file.delete();
        return;
      }
      if (value.length > 8_192) throw new Error(t('nativeSurfaces.tooManyPreferences'));
      file.write(value);
    },
    widget(content) {
      publishWidgetCatalog(widget, content);
    },
    async catalogs(items, chats) {
      const results = await Promise.allSettled([
        AppIntents.setEntityCatalogAsync('zoen.item', items),
        AppIntents.setEntityCatalogAsync('zoen.chat', chats),
      ]);
      if (results.some((result) => result.status === 'rejected'))
        throw new Error(t('nativeSurfaces.thisDeviceSShortcutsCouldNotBeUpdated'));
    },
    activities: () => activity.getInstances().map((value) => value.getId()),
    startActivity(content: MiniappActivityContent, owner, itemId) {
      const linkId = randomUUID();
      // ActivityKit attributes (including this URL) are immutable. Content and
      // item versions can update without invalidating this selected session.
      const deepLink = surfaceActivityURL(owner, itemId, linkId);
      const id = activity
        .start({ ...content, deepLink }, deepLink, new Date(content.endsAt))
        .getId();
      return { id, linkId };
    },
    async updateActivity(id, content) {
      const instance = findActivity(id);
      if (!instance) throw new Error(t('nativeSurfaces.activityEnded'));
      await instance.update(content, new Date(content.endsAt));
    },
    async endActivity(id) {
      await findActivity(id)?.end('immediate');
    },
  };
}

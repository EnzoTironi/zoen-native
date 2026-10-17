import { requireOptionalNativeModule } from 'expo';
import { File, Paths } from 'expo-file-system';
import * as AppIntents from 'expo-app-intents';
import type { SurfacePort } from './coordinator';
import type { MiniappActivityContent } from './activity';
import type { NativeWidgetContent } from './snapshot';

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
  const file = new File(Paths.document, 'native-surfaces-v1.json');
  const redacted: NativeWidgetContent = {
    redacted: true,
    title: 'Zoen',
    detail: 'Escolha um miniapp no Zoen',
    bars: [],
    rows: [],
    accentHex: '#5B8DEF',
    symbol: 'lock.fill',
  };
  function findActivity(id: string) {
    return activity.getInstances().find((value) => value.getId() === id);
  }
  return {
    async readPreferences() {
      if (!file.exists || file.size > 8_192) return null;
      return file.text();
    },
    async writePreferences(value) {
      if (value === null) {
        if (file.exists) file.delete();
        return;
      }
      if (value.length > 8_192) throw new Error('Too many native surface preferences');
      file.write(value);
    },
    widget(content) {
      const now = Date.now();
      // WidgetKit owns actual refresh scheduling. A future redacted entry limits cached exposure.
      widget.updateTimeline(
        content
          ? [
              { date: new Date(now), props: content },
              {
                date: new Date(now + 15 * 60_000),
                props: { ...redacted, deepLink: content.deepLink },
              },
            ]
          : [{ date: new Date(now), props: redacted }],
      );
    },
    async catalogs(items, chats) {
      await AppIntents.setEntityCatalogAsync('zoen.item', items);
      await AppIntents.setEntityCatalogAsync('zoen.chat', chats);
    },
    activities: () => activity.getInstances().map((value) => value.getId()),
    startActivity: (content: MiniappActivityContent) =>
      activity.start(content, content.deepLink, new Date(content.endsAt)).getId(),
    async updateActivity(id, content) {
      const instance = findActivity(id);
      if (!instance) throw new Error('Activity ended');
      await instance.update(content, new Date(content.endsAt));
    },
    async endActivity(id) {
      await findActivity(id)?.end('immediate');
    },
  };
}

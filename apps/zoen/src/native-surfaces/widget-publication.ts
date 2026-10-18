import { t } from '../i18n/index.ts';
import type { NativeWidgetContent } from './snapshot';
import type { NativeWidgetCatalog, WidgetCatalogEntry } from './widget-catalog';

export type WidgetConfiguration = { miniapp: string };
export type WidgetTimelineCatalog = Readonly<{
  entries: readonly Pick<WidgetCatalogEntry, 'selectionId' | 'content'>[];
  placeholder: NativeWidgetContent;
}>;
export interface WidgetPublicationSink {
  updateTimeline(entries: { date: Date; props: WidgetTimelineCatalog }[]): void;
  setConfigurationParameterEnum(
    parameter: 'miniapp',
    options: { name: string; value: string }[],
  ): void;
}

export const WIDGET_CONTENT_LIFETIME_MS = 15 * 60_000;

export function emptyWidgetTimeline(): WidgetTimelineCatalog {
  return {
    entries: [],
    placeholder: {
      redacted: true,
      title: 'Zoen',
      detail: t('nativeSurfaces.chooseAMiniappInZoen'),
      bars: [],
      rows: [],
      accentHex: '#5B8DEF',
      symbol: 'lock.fill',
    },
  };
}

/** The extension receives only compact snapshots; configuration names stay in the editor catalog. */
export function publishWidgetCatalog(
  sink: WidgetPublicationSink,
  catalog: NativeWidgetCatalog | null,
  now = Date.now(),
): void {
  const empty = emptyWidgetTimeline();
  const options = [{ name: t('nativeSurfaces.chooseAMiniapp'), value: 'none' }];
  const current: WidgetTimelineCatalog = catalog
    ? {
        placeholder: catalog.placeholder,
        entries: catalog.entries.map(({ selectionId, content }) => ({ selectionId, content })),
      }
    : empty;
  const timeline = [{ date: new Date(now), props: current }];
  if (catalog?.entries.length) {
    options.push(
      ...catalog.entries.map(({ selectionId, configurationTitle }) => ({
        name: configurationTitle,
        value: selectionId,
      })),
    );
    timeline.push({
      date: new Date(now + WIDGET_CONTENT_LIFETIME_MS),
      props: {
        placeholder: current.placeholder,
        entries: current.entries.map(({ selectionId, content }) => ({
          selectionId,
          content: {
            ...empty.placeholder,
            detail: t('nativeSurfaces.openToUpdate'),
            deepLink: content.deepLink,
          },
        })),
      },
    });
  }
  try {
    sink.updateTimeline(timeline);
    sink.setConfigurationParameterEnum('miniapp', options);
  } catch (failure) {
    // Both SDK stores must be cleared even when one publication fails.
    try {
      sink.updateTimeline([{ date: new Date(now), props: empty }]);
    } catch {
      // The coordinator retains the failure and retries cleanup.
    }
    try {
      sink.setConfigurationParameterEnum('miniapp', [options[0]!]);
    } catch {
      // Configuration failure must not prevent an independent timeline cleanup.
    }
    throw failure;
  }
}

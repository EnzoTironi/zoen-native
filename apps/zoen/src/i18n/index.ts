import { access } from './catalogs/access.ts';
import { activity } from './catalogs/activity.ts';
import { agents } from './catalogs/agents.ts';
import { agentEnrollment } from './catalogs/agent-enrollment.ts';
import { auth } from './catalogs/auth.ts';
import { authProviders } from './catalogs/authProviders.ts';
import { chat } from './catalogs/chat.ts';
import { common } from './catalogs/common.ts';
import { composerKeyboard } from './catalogs/composer-keyboard.ts';
import { documents } from './catalogs/documents.ts';
import { decks } from './catalogs/decks.ts';
import { drawing } from './catalogs/drawing.ts';
import { home } from './catalogs/home.ts';
import { conversationCards } from './catalogs/conversation-cards.ts';
import { connectedApps } from './catalogs/connected-apps.ts';
import { integrity } from './catalogs/integrity.ts';
import { media } from './catalogs/media.ts';
import { voiceControls } from './catalogs/voice-controls.ts';
import { messageReactions } from './catalogs/message-reactions.ts';
import { miniappFields } from './catalogs/miniappFields.ts';
import { miniapps } from './catalogs/miniapps.ts';
import { nativeSurfaces } from './catalogs/nativeSurfaces.ts';
import { notifications } from './catalogs/notifications.ts';
import { presentation } from './catalogs/presentation.ts';
import { profile } from './catalogs/profile.ts';
import { peerInfo } from './catalogs/peer-info.ts';
import { settings } from './catalogs/settings.ts';
import { createLocalization } from './runtime.ts';
import {
  formatLocale as resolveFormatLocale,
  providerLocale as resolveProviderLocale,
  sessionLocale,
  type AppLocale,
} from '../../../../packages/zoen-ui/web/locale.ts';

// Feature owners only add their catalog import and assembly entry here.
const catalogs = {
  access,
  activity,
  agents,
  agentEnrollment,
  auth,
  authProviders,
  chat,
  common,
  composerKeyboard,
  documents,
  drawing,
  decks,
  home,
  conversationCards,
  connectedApps,
  integrity,
  media,
  voiceControls,
  messageReactions,
  miniappFields,
  miniapps,
  nativeSurfaces,
  notifications,
  presentation,
  profile,
  peerInfo,
  settings,
};
function createAppLocalization(locale: AppLocale) {
  return createLocalization(catalogs, [locale]);
}
type AppLocalization = ReturnType<typeof createAppLocalization>;
let localization: AppLocalization | undefined;

export let locale: AppLocale;
export let formatLocale: 'en-US' | 'pt-BR';
export let providerLocale: Readonly<ReturnType<typeof resolveProviderLocale>>;
export let t: AppLocalization['t'] = () => {
  throw new Error('Localization must initialize before the app loads.');
};

/** One immutable language owner per JS runtime; never persist it in an account. */
export function initializeLocalization(value: AppLocale): void {
  const selected = sessionLocale(value);
  if (localization) {
    if (locale !== selected)
      throw new Error('The session language cannot change while the app is running.');
    return;
  }
  const next = createAppLocalization(selected);
  localization = next;
  locale = next.locale;
  formatLocale = resolveFormatLocale(locale);
  providerLocale = Object.freeze(resolveProviderLocale(locale));
  t = next.t;
}

import * as React from 'react';
import { createRequire } from 'node:module';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import * as authority from '../documents/document-authority.ts';
import * as access from '../home/access.ts';
import * as preview from '../media/contact-preview-session.ts';
import * as locationPreview from '../media/location-preview-session.ts';
import type { ContactPreview, LocalMediaFile, MediaDriver } from '../media/types';
const require = createRequire(import.meta.url);
compileExpoSource(new URL('../media/contact-attachment.tsx', import.meta.url));
compileExpoSource(new URL('../media/location-attachment.tsx', import.meta.url));
export async function contactSaveReactFixture(kind: 'contact' | 'location' = 'contact') {
  const f = await memberHookReactFixture();
  const buttons = new Map<string, any>();
  const texts = new Map<object, unknown>();
  let parsed: ContactPreview = {
    name: 'Ana María de Souza 李',
    phones: [
      { label: 'mobile', value: '+55 31 1234 5678' },
      { label: 'work', value: '+55 11 2345 6789' },
    ],
  };
  const file: LocalMediaFile = {
    uri: 'file:///synthetic/authorized-contact.vcf',
    itemId: 'shared-contact',
    version: 2,
    info: {
      name: 'contact.vcf',
      mime: kind === 'contact' ? 'text/vcard' : 'application/geo+json',
      sha256: 'a'.repeat(64),
      path: '',
      bytes: '80',
      ready: true,
      chunks: 1,
      chunks_here: 1,
      has_thumbnail: false,
    },
  };
  const reads: string[] = [],
    released: string[] = [],
    saved: unknown[] = [];
  let save: (contact: unknown, options: unknown) => Promise<boolean> = async (contact, options) => {
    saved.push(structuredClone({ contact, options }));
    return true;
  };
  let read: () => Promise<ContactPreview> = async () => structuredClone(parsed);
  const media: MediaDriver = {
    fileInfo: async () => file.info,
    materializeFile: async () => structuredClone(file),
    readLocation: async () => {
      reads.push(file.uri);
      return { longitude: -43.95, latitude: -19.92, label: 'Praça Nova' };
    },
    readContact: async () => {
      reads.push(file.uri);
      return read();
    },
    releaseLocalFile: async (uri) => {
      released.push(uri);
    },
    sendFile: async () => {
      throw Error('preview must not send');
    },
    sendVoice: async () => {
      throw Error('preview must not record');
    },
  };
  f.store.driver.media = media;
  function View(props: any) {
    return props.children;
  }
  function Text(props: any) {
    const id = React.useMemo(() => ({}), []);
    React.useLayoutEffect(() => {
      texts.set(id, props.children);
      return () => {
        texts.delete(id);
      };
    }, [id, props.children]);
    return null;
  }
  function Control(props: any) {
    const id = props.accessibilityLabel ?? props.label;
    React.useLayoutEffect(() => {
      buttons.set(id, props);
      return () => {
        if (buttons.get(id) === props) buttons.delete(id);
      };
    }, [id, props]);
    return null;
  }
  const contacts = {
    Contact: { presentCreateForm: (contact: unknown, options: unknown) => save(contact, options) },
  };
  // Actual platform adapter and compiled route/hooks; only SDK/UI/IO are ports.
  const adapter = loadEditorAgent('../media/contact-save.ios.ts', { 'expo-contacts': contacts });
  const ports = {
    ...f.ports,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      ...(f.ports['react-native'] as object),
      View,
      ScrollView: View,
      Text,
      ActivityIndicator: () => null,
    },
    '../components/primitives': { Icon: () => null, IconButton: Control, NativeButton: Control },
    '../components/message-interaction': { MessagePressable: Control },
    '../documents/document-authority': authority,
    '../home/access': access,
    './contact-preview-session': preview,
    './location-preview-session': locationPreview,
    './location-map': { LocationMap: () => null },
    './media-sheet': { MediaSheet: View },
    './contact-save': adapter,
    '../theme': { useTheme: () => ({ colors: {} }) },
    '../i18n': { t: (key: string) => key },
    'expo-sharing': { isAvailableAsync: async () => true, shareAsync: async () => {} },
    'expo-clipboard': { setStringAsync: async () => true },
  };
  const fileHook = loadEditorAgent('../media/use-file-preview.ts', ports);
  const contactHook = loadEditorAgent('../media/use-contact-preview.ts', {
    ...ports,
    './use-file-preview': fileHook,
  });
  const component = loadEditorAgent(`../media/${kind}-attachment.tsx`, {
    ...ports,
    './use-contact-preview': contactHook,
    './use-file-preview': fileHook,
  });
  return {
    ...f,
    buttons,
    reads,
    released,
    saved,
    parsed,
    text: () => [...texts.values()].flat().join(' '),
    save: (next: typeof save) => {
      save = next;
    },
    read: (next: typeof read) => {
      read = next;
    },
    async renderContact() {
      await f.renderContent(
        React.createElement(
          component[kind === 'contact' ? 'ContactAttachment' : 'LocationAttachment'],
          {
            media,
            source: { itemId: file.itemId, version: file.version, expectedHash: file.info.sha256 },
            spaceId: f.snapshot.conversations[0]!.id,
            messageId: 'original-message',
            readiness: '80 B',
            message: true,
            isCurrent: () => true,
          },
        ),
      );
    },
    async openContact() {
      const button = [...buttons.values()].find((value) => value.accessibilityRole === 'button');
      if (!button) throw Error('contact card missing');
      await f.act(() => {
        button.onPress();
      });
    },
    async pressSave() {
      const button = buttons.get('media.saveContact');
      if (!button || button.disabled) throw Error('contact save unavailable');
      await f.act(() => {
        button.onPress();
      });
    },
  };
}

import { Host } from '@expo/ui';
import { Button, Menu } from '@expo/ui/swift-ui';
import { buttonStyle, foregroundStyle, frame, labelStyle, tint } from '@expo/ui/swift-ui/modifiers';
import { locale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { useNativeCalls } from './provider';

/** Tap calls; press and hold offers the same native voice/video choices. */
export function CallHeaderAction({ space }: { space: string }) {
  const calls = useNativeCalls();
  const { inform } = useDialogs();
  const { scheme, presentation: p } = useTheme();
  const pt = locale === 'pt-BR';
  const start = (video: boolean) => {
    void calls
      .start(space, video)
      .catch(() =>
        inform(
          pt ? 'Não foi possível iniciar a chamada' : 'Could not start the call',
          pt ? 'Verifique as permissões e tente novamente.' : 'Check permissions and try again.',
        ),
      );
  };
  return (
    <Host style={{ width: 44, height: 44 }} colorScheme={scheme} ignoreSafeArea="all">
      <Menu
        testID="chat-call"
        label={pt ? 'Ligar' : 'Call'}
        systemImage="phone"
        onPrimaryAction={() => start(false)}
        modifiers={[
          buttonStyle('plain'),
          labelStyle('iconOnly'),
          foregroundStyle(p.navigation.control),
          tint(p.navigation.control),
          frame({ width: 44, height: 44 }),
        ]}
      >
        <Button
          label={pt ? 'Chamada de voz' : 'Voice call'}
          systemImage="phone"
          onPress={() => start(false)}
        />
        <Button
          label={pt ? 'Chamada de vídeo' : 'Video call'}
          systemImage="video"
          onPress={() => start(true)}
        />
      </Menu>
    </Host>
  );
}

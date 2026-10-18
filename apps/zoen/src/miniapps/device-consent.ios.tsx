import { Host } from '@expo/ui';
import { useState } from 'react';
import { BottomSheet, Button, Group, ScrollView, Text, VStack } from '@expo/ui/swift-ui';
import {
  accessibilityAddTraits,
  buttonStyle,
  contentShape,
  disabled,
  font,
  foregroundStyle,
  frame,
  onAppear,
  padding,
  preferredColorScheme,
  presentationBackground,
  presentationDetents,
  presentationDragIndicator,
  shapes,
  textSelection,
  tint,
} from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { ConsentChoice } from './device-consent-queue';
import type { DeviceConsentProps } from './device-consent.types';

/** The native sheet owns dismissal; a choice alone never starts an OS prompt. */
export function DeviceConsent(props: DeviceConsentProps) {
  const { colors, scheme } = useTheme();
  const request = props.request;
  const [appeared, setAppeared] = useState<number | null>(null);
  if (!request) return null;
  const actions: { choice: ConsentChoice; label: string }[] = [
    { choice: 'once', label: t('miniapps.allowOnce') },
    { choice: 'always', label: t('miniapps.allowUntilIRevoke') },
    { choice: 'deny', label: t('miniapps.doNotAllow') },
  ];
  return (
    <Host
      key={request.id}
      testID="device-consent-host"
      style={{ position: 'absolute', width: 1, height: 1 }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <BottomSheet
        testID="device-consent-sheet"
        // SwiftUI emits no dismissal when true→false is coalesced before its
        // first appearance. Obtain a real presentation before retiring it.
        // A pre-appearance cancellation stays denied and its controls disabled.
        isPresented={props.phase === 'presented' || appeared !== request.id}
        onIsPresentedChange={(presented) => {
          if (!presented) props.onChoose(request.id, 'deny');
        }}
        onDismiss={() => props.onDismiss(request.id)}
      >
        <Group
          modifiers={[
            // Request IDs increase across owners; a late old appearance cannot reopen a newer sheet.
            onAppear(() => setAppeared((previous) => Math.max(previous ?? 0, request.id))),
            presentationBackground(colors.background),
            preferredColorScheme(scheme),
            presentationDetents(['medium', 'large']),
            presentationDragIndicator('visible'),
          ]}
        >
          <ScrollView>
            <VStack
              alignment="leading"
              spacing={16}
              modifiers={[padding({ all: 24 }), frame({ maxWidth: Infinity })]}
            >
              <Text
                modifiers={[
                  font({ textStyle: 'title2', weight: 'semibold' }),
                  foregroundStyle(colors.text),
                  accessibilityAddTraits(['isHeader']),
                ]}
              >
                {props.title}
              </Text>
              <Text
                modifiers={[
                  font({ textStyle: 'body' }),
                  foregroundStyle(colors.text),
                  textSelection(true),
                ]}
              >
                {request.purpose}
              </Text>
              <Text modifiers={[font({ textStyle: 'callout' }), foregroundStyle(colors.secondary)]}>
                {props.scope}
              </Text>
              <VStack spacing={8} modifiers={[frame({ maxWidth: Infinity })]}>
                {actions.map(({ choice, label }) => (
                  <Button
                    key={choice}
                    testID={`device-consent-${choice}`}
                    role={choice === 'deny' ? 'cancel' : 'default'}
                    onPress={() => props.onChoose(request.id, choice)}
                    modifiers={[
                      buttonStyle(choice === 'once' ? 'borderedProminent' : 'bordered'),
                      tint(colors.accent),
                      foregroundStyle(choice === 'once' ? colors.onAccent : colors.text),
                      disabled(props.phase !== 'presented'),
                    ]}
                  >
                    <Text
                      modifiers={[
                        font({
                          textStyle: 'body',
                          weight: choice === 'once' ? 'semibold' : 'regular',
                        }),
                        frame({ minHeight: 44, maxWidth: Infinity }),
                        contentShape(shapes.rectangle(), ['interaction', 'accessibility']),
                      ]}
                    >
                      {label}
                    </Text>
                  </Button>
                ))}
              </VStack>
            </VStack>
          </ScrollView>
        </Group>
      </BottomSheet>
    </Host>
  );
}

import { Text, View, type ColorValue } from 'react-native';
import { Field, NativeButton } from '../components/primitives';
import { t } from '../i18n';

type PollFields = {
  question: string;
  options: string[];
  locked: boolean;
  unknownPoll: boolean;
  colors: { secondary: ColorValue };
  current(): boolean;
  working: { current: boolean };
  pollAttempted: { current: boolean };
  setQuestion(value: string): void;
  setOptions(value: string[]): void;
  perform(work: () => Promise<void>): Promise<void>;
  createPoll(): Promise<void>;
};

/** Stateless fields; route admission and once-only mutation ownership stay with the route. */
export function renderPollAttachmentFields({
  question,
  options,
  locked,
  unknownPoll,
  colors,
  current,
  working,
  pollAttempted,
  setQuestion,
  setOptions,
  perform,
  createPoll,
}: PollFields) {
  return (
            <>
              <Field
                label={t('attachmentTabs.question')}
                value={question}
                editable={!locked && !unknownPoll}
                onChangeText={(value) => {
                  if (current() && !working.current && !pollAttempted.current) setQuestion(value);
                }}
              />
              {options.map((option, index) => (
                <View key={index} style={{ gap: 8 }}>
                  <Field
                    label={t('attachmentTabs.option', { number: index + 1 })}
                    value={option}
                    editable={!locked && !unknownPoll}
                    onChangeText={(value) => {
                      if (current() && !working.current && !pollAttempted.current)
                        setOptions(
                          options.map((old, position) => (position === index ? value : old)),
                        );
                    }}
                  />
                  {options.length > 2 ? (
                    <NativeButton
                      secondary
                      label={t('attachmentTabs.removeOption', { number: index + 1 })}
                      disabled={locked || unknownPoll}
                      onPress={() => {
                        if (current() && !working.current && !pollAttempted.current)
                          setOptions(options.filter((_, position) => position !== index));
                      }}
                    />
                  ) : null}
                </View>
              ))}
              <NativeButton
                secondary
                label={t('attachmentTabs.addOption')}
                disabled={locked || unknownPoll || options.length >= 10}
                onPress={() => {
                  if (
                    current() &&
                    !working.current &&
                    !pollAttempted.current &&
                    options.length < 10
                  )
                    setOptions([...options, '']);
                }}
              />
              <NativeButton
                label={t('attachmentTabs.createPoll')}
                disabled={locked || unknownPoll}
                onPress={() => {
                  void perform(createPoll);
                }}
              />
              {unknownPoll ? (
                <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
                  {t('attachmentTabs.pollUnknown')}
                </Text>
              ) : null}
            </>
  );
}

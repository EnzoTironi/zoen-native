import { View } from 'react-native';
import { Field, NativeButton } from '../components/primitives';
import { t } from '../i18n';
import type { MediaDraftPollOption } from './media-draft-types';

type PollFields = {
  question: string;
  options: readonly MediaDraftPollOption[];
  locked: boolean;
  sending: boolean;
  retry: boolean;
  setQuestion(value: string): void;
  setOption(id: string, value: string): void;
  addOption(): void;
  removeOption(id: string): void;
  submit(): void;
};

/** Stable private option IDs keep queued native callbacks attached to the same field. */
export function PollAttachmentFields(props: PollFields) {
  return (
    <>
      <Field
        label={t('attachmentTabs.question')}
        value={props.question}
        editable={!props.locked}
        onChangeText={props.setQuestion}
      />
      {props.options.map((option, index) => (
        <View key={option.id} style={{ gap: 8 }}>
          <Field
            label={t('attachmentTabs.option', { number: index + 1 })}
            value={option.text}
            editable={!props.locked}
            onChangeText={(value) => props.setOption(option.id, value)}
          />
          {props.options.length > 2 ? (
            <NativeButton
              secondary
              label={t('attachmentTabs.removeOption', { number: index + 1 })}
              disabled={props.locked}
              onPress={() => props.removeOption(option.id)}
            />
          ) : null}
        </View>
      ))}
      <NativeButton
        secondary
        label={t('attachmentTabs.addOption')}
        disabled={props.locked || props.options.length >= 10}
        onPress={props.addOption}
      />
      <NativeButton
        label={t(props.retry ? 'chat.tryAgain2' : 'attachmentTabs.createPoll')}
        disabled={props.sending || (props.locked && !props.retry)}
        onPress={props.submit}
      />
    </>
  );
}

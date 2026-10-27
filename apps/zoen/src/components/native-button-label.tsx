import { Text } from '@expo/ui';

export function FullWidthButtonLabel({ label }: { label: string }) {
  return (
    <Text style={{ width: '100%' }} textStyle={{ textAlign: 'center' }}>
      {label}
    </Text>
  );
}

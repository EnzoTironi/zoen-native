import { requireNativeView } from 'expo';
import type { ViewProps } from 'react-native';

export type PageInputViewProps = ViewProps & { token: string };
const NativeInput = requireNativeView<PageInputViewProps>('ZoenCore');
export function PageInputView(props: PageInputViewProps) {
  return <NativeInput {...props} />;
}

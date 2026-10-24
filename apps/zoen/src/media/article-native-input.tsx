import { useImperativeHandle, useRef } from 'react';
import {
  EnrichedMarkdownTextInput,
  type EnrichedMarkdownTextInputInstance,
  type EnrichedMarkdownTextInputProps,
} from 'react-native-enriched-markdown';

/** ENRM1.1.1 refreshes its JS facade on each render. Its commands close over the
 * same Fabric node/request refs, so expose one actual facade per mounted input.
 * React releases this ref on unmount; a replacement input gets a new identity.
 */
export function ArticleNativeInput({ ref, ...props }: EnrichedMarkdownTextInputProps) {
  const input = useRef<EnrichedMarkdownTextInputInstance>(null);
  useImperativeHandle(ref, () => {
    const editor = input.current;
    if (!editor) throw new Error('Article native input is not mounted');
    return editor;
  }, []);
  return <EnrichedMarkdownTextInput {...props} ref={input} />;
}

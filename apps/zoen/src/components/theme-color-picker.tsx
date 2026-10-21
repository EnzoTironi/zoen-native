import { useState } from 'react';
import { TextInput } from 'react-native';
import { useTheme } from '../theme';

export function ThemeColorPicker({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange(value: string): void;
}) {
  const { presentation } = useTheme();
  const [input, setInput] = useState({ value, draft: value });
  if (input.value !== value) setInput({ value, draft: value });
  return (
    <TextInput
      accessibilityLabel={label}
      value={input.draft}
      autoCapitalize="characters"
      autoCorrect={false}
      maxLength={7}
      selectTextOnFocus
      onChangeText={(draft) => setInput({ value, draft })}
      onEndEditing={() => {
        if (/^#[\da-f]{6}$/i.test(input.draft)) onChange(input.draft.toUpperCase());
        else setInput({ value, draft: value });
      }}
      style={{ minHeight: 44, width: 92, color: presentation.text, fontSize: 15 }}
    />
  );
}

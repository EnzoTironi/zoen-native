import { TextInput, View, Text } from 'react-native';
import { useTheme } from '../theme';
export function BlockEditor({
  initial,
  onChange,
  disabled,
}: {
  initial: string;
  onChange: (markdown: string) => void;
  disabled: boolean;
  sourceOnly?: boolean;
}) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 12 }}>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>
        Markdown · **negrito**, *itálico*, [texto](https://…)
      </Text>
      <TextInput
        defaultValue={initial}
        onChangeText={onChange}
        editable={!disabled}
        autoFocus
        multiline
        accessibilityLabel="Conteúdo do bloco em Markdown"
        style={{
          minHeight: 230,
          padding: 18,
          color: colors.text,
          backgroundColor: colors.surface,
          borderRadius: 18,
          fontSize: 17,
          lineHeight: 27,
          fontFamily: 'monospace',
        }}
      />
    </View>
  );
}

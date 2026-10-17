import { Pressable, Text, View } from 'react-native';
import { Field, NativeButton, Surface } from '../components/primitives';
import { useTheme } from '../theme';
import { draftObject, fieldLabel, isDateField, isPriceField, type Schema } from './schema';

const weekdays = ['Domingo', 'Segunda', 'Terça', 'Quarta', 'Quinta', 'Sexta', 'Sábado'];
export function SchemaForm({
  schema,
  value,
  onChange,
  disabled,
  name = '',
  required = true,
  depth = 0,
}: {
  schema: Schema;
  value: unknown;
  onChange: (value: unknown) => void;
  disabled: boolean;
  name?: string;
  required?: boolean;
  depth?: number;
}) {
  const { colors } = useTheme();
  const label = fieldLabel(name, schema);
  if (depth > 8)
    return (
      <Text style={{ color: colors.danger }}>
        Este formulário é complexo demais para ser aberto.
      </Text>
    );
  if (schema.enum)
    return (
      <View style={{ gap: 8 }}>
        <Text style={{ color: colors.secondary, fontWeight: '600' }}>
          {label}
          {required ? ' *' : ''}
        </Text>
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {schema.enum.map((option) => (
            <Pressable
              key={String(option)}
              disabled={disabled}
              accessibilityRole="radio"
              accessibilityState={{ checked: value === option, disabled }}
              onPress={() => onChange(option)}
              style={{
                minHeight: 44,
                paddingHorizontal: 16,
                justifyContent: 'center',
                borderRadius: 20,
                backgroundColor: value === option ? colors.accent : colors.muted,
              }}
            >
              <Text style={{ color: value === option ? colors.onAccent : colors.text }}>
                {String(option)}
              </Text>
            </Pressable>
          ))}
        </View>
      </View>
    );
  if (name === 'weeklyHours') {
    const hours = draftObject(value);
    return (
      <Surface>
        <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
          Horários de atendimento
        </Text>
        {weekdays.map((day, index) => {
          const windows = Array.isArray(hours[String(index)])
            ? (hours[String(index)] as unknown[])
            : [];
          return (
            <View key={day} style={{ gap: 10, paddingVertical: 8 }}>
              <Pressable
                disabled={disabled}
                accessibilityRole="checkbox"
                accessibilityState={{ checked: windows.length > 0, disabled }}
                onPress={() => {
                  const next = { ...hours };
                  if (windows.length) delete next[String(index)];
                  else next[String(index)] = [['09:00', '18:00']];
                  onChange(next);
                }}
                style={{ minHeight: 44, justifyContent: 'center' }}
              >
                <Text style={{ color: colors.text, fontWeight: '600' }}>
                  {windows.length ? '✓' : '○'} {day}
                </Text>
              </Pressable>
              {windows.length ? (
                <SchemaForm
                  name={day}
                  schema={{
                    type: 'array',
                    maxItems: 4,
                    items: { type: 'array', minItems: 2, maxItems: 2, items: { type: 'string' } },
                  }}
                  value={windows}
                  onChange={(next) => onChange({ ...hours, [String(index)]: next })}
                  disabled={disabled}
                  depth={depth + 1}
                />
              ) : null}
            </View>
          );
        })}
      </Surface>
    );
  }
  if (schema.type === 'object' || schema.properties) {
    const record = draftObject(value);
    return (
      <View style={{ gap: 18 }}>
        {Object.entries(schema.properties ?? {}).map(([key, child]) => (
          <SchemaForm
            key={key}
            name={key}
            schema={child}
            value={record[key]}
            onChange={(next) => onChange({ ...record, [key]: next })}
            disabled={disabled}
            required={schema.required?.includes(key) ?? false}
            depth={depth + 1}
          />
        ))}
      </View>
    );
  }
  if (schema.type === 'array') {
    const entries = Array.isArray(value) ? value : [];
    const fixed = schema.minItems !== undefined && schema.minItems === schema.maxItems;
    const visible =
      fixed && !entries.length ? (Array(schema.minItems).fill('') as unknown[]) : entries;
    return (
      <View style={{ gap: 12 }}>
        <Text style={{ color: colors.secondary, fontWeight: '600' }}>
          {label}
          {required ? ' *' : ''}
        </Text>
        {visible.map((entry, index) => (
          <Surface key={index}>
            <SchemaForm
              schema={schema.items ?? { type: 'string' }}
              name={schema.items?.type === 'object' ? '' : `${label} ${index + 1}`}
              required
              value={entry}
              disabled={disabled}
              depth={depth + 1}
              onChange={(next) => {
                const values = [...visible];
                values[index] = next;
                onChange(values);
              }}
            />
            {!fixed ? (
              <NativeButton
                secondary
                label={`Remover ${index + 1}`}
                disabled={disabled}
                onPress={() => onChange(visible.filter((_, position) => position !== index))}
              />
            ) : null}
          </Surface>
        ))}
        {!fixed ? (
          <NativeButton
            secondary
            label={`Adicionar em ${label.toLowerCase()}`}
            disabled={disabled || visible.length >= (schema.maxItems ?? 24)}
            onPress={() =>
              onChange([
                ...visible,
                schema.items?.type === 'object'
                  ? {}
                  : schema.items?.type === 'array'
                    ? Array(schema.items.minItems ?? 0).fill('')
                    : '',
              ])
            }
          />
        ) : null}
      </View>
    );
  }
  if (schema.type === 'boolean')
    return (
      <Pressable
        disabled={disabled}
        accessibilityRole="checkbox"
        accessibilityState={{ checked: value === true, disabled }}
        onPress={() => onChange(value !== true)}
        style={{ minHeight: 44, justifyContent: 'center' }}
      >
        <Text style={{ color: colors.text }}>
          {value === true ? '✓' : '○'} {label}
        </Text>
      </Pressable>
    );
  return (
    <View style={{ gap: 6 }}>
      <Field
        label={`${label}${required ? ' *' : ''}`}
        value={typeof value === 'string' ? value : ''}
        onChangeText={onChange}
        editable={!disabled}
        keyboardType={
          isPriceField(name)
            ? 'decimal-pad'
            : schema.type === 'integer' && !isDateField(name)
              ? 'numbers-and-punctuation'
              : schema.type === 'number'
                ? 'decimal-pad'
                : 'default'
        }
        autoCapitalize={
          isDateField(name) || name === 'timezone' || name === 'currency' ? 'none' : 'sentences'
        }
        placeholder={
          isDateField(name) ? '2026-10-20 09:00' : isPriceField(name) ? '100,00' : undefined
        }
      />
      {isDateField(name) ? (
        <Text style={{ color: colors.secondary, fontSize: 12 }}>
          Data e hora local: ano-mês-dia hora:minuto.
        </Text>
      ) : isPriceField(name) ? (
        <Text style={{ color: colors.secondary, fontSize: 12 }}>
          Valor em reais, com vírgula para os centavos.
        </Text>
      ) : null}
    </View>
  );
}

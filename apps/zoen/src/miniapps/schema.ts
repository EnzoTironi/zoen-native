import { parseMoneyInput } from '../core/presentation.ts';

export type Schema = {
  type?: string;
  title?: string;
  description?: string;
  enum?: unknown[];
  properties?: Record<string, Schema>;
  patternProperties?: Record<string, Schema>;
  items?: Schema;
  required?: string[];
  minimum?: number;
  maximum?: number;
  minItems?: number;
  maxItems?: number;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  additionalProperties?: boolean;
};
export type ExactJson =
  | null
  | string
  | number
  | boolean
  | bigint
  | ExactJson[]
  | { [key: string]: ExactJson };
export function parseSchema(encoded: string): Schema {
  const value: unknown = JSON.parse(encoded);
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    ('type' in value && value.type !== 'object')
  )
    throw new Error('O formulário deste miniapp não está disponível.');
  return value as Schema;
}
export function isDateField(key: string): boolean {
  return ['target_ms', 'startMs', 'endMs'].includes(key);
}
export function isPriceField(key: string): boolean {
  return /Cents$|_cents$/.test(key);
}
const names: Record<string, string> = {
  name: 'Nome',
  title: 'Título',
  business: 'Negócio',
  description: 'Descrição',
  species: 'Espécie',
  question: 'Pergunta',
  options: 'Opções',
  items: 'Itens',
  dish: 'Prato',
  servings: 'Porções',
  day: 'Dia',
  area: 'Região',
  driver: 'Quem dirige',
  pickups: 'Pontos de encontro',
  names: 'Pessoas',
  place: 'Local',
  photos: 'Fotos',
  target_ms: 'Data e hora',
  startMs: 'Início',
  endMs: 'Fim',
  serviceId: 'Serviço',
  bookingId: 'Agendamento',
  note: 'Observação',
  action: 'Decisão',
  timezone: 'Fuso horário',
  currency: 'Moeda',
  services: 'Serviços',
  minutes: 'Duração em minutos',
  priceCents: 'Preço em reais',
  slotMinutes: 'Intervalo em minutos',
  horizonDays: 'Dias disponíveis',
  weeklyHours: 'Horários de atendimento',
  accentHex: 'Cor',
  resolvers: 'Quem pode resolver pedidos',
  forId: 'Pessoa atendida',
  forName: 'Nome da pessoa',
  round: 'Rodada',
  lat: 'Latitude',
  lon: 'Longitude',
  meters: 'Metros percorridos',
  carrots: 'Cenouras',
  option: 'Opção',
  text: 'Texto',
  trail: 'Trilha',
  id: 'Identificação',
};
export function fieldLabel(key: string, schema: Schema): string {
  return schema.title ?? names[key] ?? key.replaceAll('_', ' ').replace(/([a-z])([A-Z])/g, '$1 $2');
}
export function draftObject(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

/** Schema validation feeds Rust decimal integers, including values above 2^53. */
export function formValue(
  schema: Schema,
  draft: unknown,
  key = '',
  required = true,
  depth = 0,
): ExactJson | undefined {
  const label = fieldLabel(key, schema) || 'Campo';
  if (depth > 8) throw new Error('Este formulário é complexo demais para ser aberto.');
  const empty = draft === undefined || draft === '' || draft === null;
  if (empty && !required) return undefined;
  if (schema.enum) {
    if (!schema.enum.some((value) => value === draft))
      throw new Error(`Escolha ${label.toLowerCase()}.`);
    return draft as string | number | boolean | null;
  }
  if (schema.type === 'object' || schema.properties || schema.patternProperties) {
    const record = draftObject(draft);
    const result: Record<string, ExactJson> = Object.create(null) as Record<string, ExactJson>;
    const keys = new Set([...Object.keys(schema.properties ?? {}), ...Object.keys(record)]);
    for (const property of keys) {
      const child =
        schema.properties?.[property] ??
        Object.entries(schema.patternProperties ?? {}).find(([pattern]) =>
          new RegExp(pattern).test(property),
        )?.[1];
      if (!child) {
        if (schema.additionalProperties === false)
          throw new Error(`Campo desconhecido: ${property}.`);
        continue;
      }
      const value = formValue(
        child,
        record[property],
        property,
        schema.required?.includes(property) ?? false,
        depth + 1,
      );
      if (value !== undefined) result[property] = value;
    }
    if (!Object.keys(result).length && !required) return undefined;
    return result;
  }
  if (schema.type === 'array') {
    const list = Array.isArray(draft) ? draft : [];
    if (list.length < (schema.minItems ?? 0))
      throw new Error(`Adicione pelo menos ${schema.minItems} em ${label.toLowerCase()}.`);
    if (list.length > (schema.maxItems ?? 100))
      throw new Error(`Há itens demais em ${label.toLowerCase()}.`);
    return list.map(
      (value) => formValue(schema.items ?? { type: 'string' }, value, key, true, depth + 1) ?? null,
    );
  }
  if (schema.type === 'boolean') return draft === true;
  if (schema.type === 'integer' || schema.type === 'number') {
    const input = String(draft ?? '').trim();
    let value: bigint | number;
    if (isPriceField(key)) value = BigInt(parseMoneyInput(input));
    else if (isDateField(key)) {
      const milliseconds = Date.parse(input.replace(' ', 'T'));
      if (!Number.isSafeInteger(milliseconds))
        throw new Error(`Informe uma data válida em ${label.toLowerCase()}.`);
      value = BigInt(milliseconds);
    } else if (schema.type === 'integer') {
      if (!/^-?\d+$/.test(input) || input.length > 20)
        throw new Error(`Informe um número inteiro em ${label.toLowerCase()}.`);
      value = BigInt(input);
      if (value < -9223372036854775808n || value > 9223372036854775807n)
        throw new Error(`${label} ultrapassa o limite permitido.`);
    } else {
      if (!/^-?(?:\d+(?:[.,]\d*)?|[.,]\d+)$/.test(input))
        throw new Error(`Informe um número em ${label.toLowerCase()}.`);
      value = Number(input.replace(',', '.'));
      if (!Number.isFinite(value)) throw new Error(`${label} ultrapassa o limite permitido.`);
    }
    if (schema.minimum !== undefined && value < schema.minimum)
      throw new Error(`${label} precisa ser pelo menos ${schema.minimum}.`);
    if (schema.maximum !== undefined && value > schema.maximum)
      throw new Error(`${label} precisa ser no máximo ${schema.maximum}.`);
    return value;
  }
  const value = typeof draft === 'string' ? draft : '';
  const count = Array.from(value).length;
  if (count < (schema.minLength ?? (required ? 1 : 0)))
    throw new Error(`Preencha ${label.toLowerCase()}.`);
  if (count > (schema.maxLength ?? 16_384)) throw new Error(`${label} está longo demais.`);
  if (schema.pattern && !new RegExp(schema.pattern).test(value))
    throw new Error(`Confira o formato de ${label.toLowerCase()}.`);
  return value;
}

/** JSON numbers remain exact decimal tokens; BigInt never crosses the bridge itself. */
export function exactJson(value: ExactJson): string {
  if (typeof value === 'bigint') return value.toString();
  if (Array.isArray(value)) return `[${value.map(exactJson).join(',')}]`;
  if (value && typeof value === 'object')
    return `{${Object.entries(value)
      .map(([key, value]) => `${JSON.stringify(key)}:${exactJson(value)}`)
      .join(',')}}`;
  return JSON.stringify(value);
}

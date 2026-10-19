import { t } from '../i18n/index.ts';
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
    throw new Error(t('miniapps.thisMiniappSFormIsNotAvailable'));
  return value as Schema;
}
export function isDateField(key: string): boolean {
  return ['target_ms', 'startMs', 'endMs'].includes(key);
}
export function isPriceField(key: string): boolean {
  return /Cents$|_cents$/.test(key);
}
const names: Record<string, string> = {
  name: t('miniappFields.name'),
  title: t('miniappFields.title'),
  business: t('miniappFields.business'),
  description: t('miniappFields.description'),
  species: t('miniappFields.species'),
  question: t('miniappFields.question'),
  options: t('miniappFields.options'),
  items: t('miniappFields.items'),
  dish: t('miniappFields.dish'),
  servings: t('miniappFields.servings'),
  day: t('miniappFields.day'),
  area: t('miniappFields.area'),
  driver: t('miniappFields.driver'),
  pickups: t('miniappFields.pickups'),
  names: t('miniappFields.names'),
  place: t('miniappFields.place'),
  photos: t('miniappFields.photos'),
  target_ms: t('miniappFields.target_ms'),
  startMs: t('miniappFields.startMs'),
  endMs: t('miniappFields.endMs'),
  serviceId: t('miniappFields.serviceId'),
  bookingId: t('miniappFields.bookingId'),
  note: t('miniappFields.note'),
  action: t('miniappFields.action'),
  timezone: t('miniappFields.timezone'),
  currency: t('miniappFields.currency'),
  services: t('miniappFields.services'),
  minutes: t('miniappFields.minutes'),
  priceCents: t('miniappFields.priceCents'),
  slotMinutes: t('miniappFields.slotMinutes'),
  horizonDays: t('miniappFields.horizonDays'),
  weeklyHours: t('miniappFields.weeklyHours'),
  accentHex: t('miniappFields.accentHex'),
  resolvers: t('miniappFields.resolvers'),
  forId: t('miniappFields.forId'),
  forName: t('miniappFields.forName'),
  round: t('miniappFields.round'),
  lat: t('miniappFields.lat'),
  lon: t('miniappFields.lon'),
  meters: t('miniappFields.meters'),
  carrots: t('miniappFields.carrots'),
  option: t('miniappFields.option'),
  text: t('miniappFields.text'),
  trail: t('miniappFields.trail'),
  id: t('miniappFields.id'),
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
  const label = fieldLabel(key, schema) || t('miniapps.field');
  if (depth > 8) throw new Error(t('miniapps.thisFormIsTooComplexToOpen'));
  const empty = draft === undefined || draft === '' || draft === null;
  if (empty && !required) return undefined;
  if (schema.enum) {
    if (!schema.enum.some((value) => value === draft))
      throw new Error(t('miniapps.chooseField', { label }));
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
          throw new Error(t('miniapps.unknownField', { field: property }));
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
      throw new Error(t('miniapps.minimumItems', { count: schema.minItems ?? 0, label }));
    if (list.length > (schema.maxItems ?? 100))
      throw new Error(t('miniapps.tooManyItems', { label }));
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
      if (!Number.isSafeInteger(milliseconds)) throw new Error(t('miniapps.validDate', { label }));
      value = BigInt(milliseconds);
    } else if (schema.type === 'integer') {
      if (!/^-?\d+$/.test(input) || input.length > 20)
        throw new Error(t('miniapps.validInteger', { label }));
      value = BigInt(input);
      if (value < -9223372036854775808n || value > 9223372036854775807n)
        throw new Error(t('miniapps.fieldAboveLimit', { label }));
    } else {
      if (!/^-?(?:\d+(?:[.,]\d*)?|[.,]\d+)$/.test(input))
        throw new Error(t('miniapps.validNumber', { label }));
      value = Number(input.replace(',', '.'));
      if (!Number.isFinite(value)) throw new Error(t('miniapps.fieldAboveLimit', { label }));
    }
    if (schema.minimum !== undefined && value < schema.minimum)
      throw new Error(t('miniapps.fieldMinimum', { label, minimum: schema.minimum }));
    if (schema.maximum !== undefined && value > schema.maximum)
      throw new Error(t('miniapps.fieldMaximum', { label, maximum: schema.maximum }));
    return value;
  }
  const value = typeof draft === 'string' ? draft : '';
  const count = Array.from(value).length;
  if (count < (schema.minLength ?? (required ? 1 : 0)))
    throw new Error(t('miniapps.requiredField', { label }));
  if (count > (schema.maxLength ?? 16_384)) throw new Error(t('miniapps.fieldTooLong', { label }));
  if (schema.pattern && !new RegExp(schema.pattern).test(value))
    throw new Error(t('miniapps.fieldFormat', { label }));
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

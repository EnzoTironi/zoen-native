import { t } from '../i18n/index.ts';
import { isInteger, isSafeNumber, LosslessNumber, parse, stringify } from 'lossless-json';

/** Large Rust integers remain decimal strings inside miniapp view_json too. */
export function parseMiniappJson(encoded: string): unknown {
  return parse(encoded, undefined, {
    parseNumber: (value) => {
      const numeric = Number(value);
      return isSafeNumber(value) && (!isInteger(value) || Number.isSafeInteger(numeric))
        ? numeric
        : value;
    },
  });
}

/** Tool arguments preserve numeric tokens; quoted numeric strings stay strings. */
export function parseMiniappRequest(encoded: string): unknown {
  return parse(encoded, undefined, {
    parseNumber: (value) => (isSafeNumber(value) ? Number(value) : new LosslessNumber(value)),
  });
}

export function stringifyMiniappRequest(value: unknown): string {
  const encoded = stringify(value);
  if (encoded === undefined) throw new Error(t('miniapps.theDataForThisActionIsInvalid'));
  return encoded;
}

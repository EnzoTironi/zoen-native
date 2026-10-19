import { t } from '../i18n/index.ts';
export function photoArguments(
  value: unknown,
  item: string,
  resolve: (token: string, item: string) => string | null,
  replace = false,
): { value: unknown; tokens: string[] } {
  const tokens = new Set<string>();
  function visit(input: unknown, depth: number): unknown {
    if (depth > 12) throw new Error(t('miniapps.theDataForThisActionIsTooComplex'));
    if (isLosslessNumber(input)) return input;
    if (typeof input === 'string') {
      if (/^data:image/i.test(input))
        throw new Error(t('miniapps.choosePhotosUsingThePickerAndConfirmBefore'));
      if (input.startsWith('zoen-photo:')) {
        const data = resolve(input, item);
        if (!data) throw new Error(t('miniapps.thisPhotoDoesNotBelongToThisMiniapp'));
        tokens.add(input);
        return replace ? data : input;
      }
      return input;
    }
    if (Array.isArray(input)) return input.map((entry) => visit(entry, depth + 1));
    if (input && typeof input === 'object')
      return Object.fromEntries(
        Object.entries(input).map(([key, entry]) => [key, visit(entry, depth + 1)]),
      );
    return input;
  }
  return { value: visit(value, 0), tokens: [...tokens] };
}
import { isLosslessNumber } from 'lossless-json';

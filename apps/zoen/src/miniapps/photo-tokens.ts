export function photoArguments(
  value: unknown,
  item: string,
  resolve: (token: string, item: string) => string | null,
  replace = false,
): { value: unknown; tokens: string[] } {
  const tokens = new Set<string>();
  function visit(input: unknown, depth: number): unknown {
    if (depth > 12) throw new Error('Os dados desta ação são complexos demais.');
    if (isLosslessNumber(input)) return input;
    if (typeof input === 'string') {
      if (/^data:image/i.test(input))
        throw new Error(
          'Escolha as fotos pelo seletor e confirme antes de compartilhar com o grupo.',
        );
      if (input.startsWith('zoen-photo:')) {
        const data = resolve(input, item);
        if (!data) throw new Error('Esta foto não pertence a este miniapp ou já foi descartada.');
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

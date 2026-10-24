import type { ImageToolReview } from './generated/ImageToolReview';

const hash = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);

/** Presentation validation only. The signed Core proposal remains authoritative. */
export function imageToolReviewValid(value: unknown): value is ImageToolReview {
  if (typeof value !== 'object' || value === null) return false;
  const image = value as Record<string, unknown>;
  return (
    typeof image.prompt === 'string' &&
    image.prompt.trim().length > 0 &&
    !image.prompt.includes('\0') &&
    Array.from(image.prompt).length <= 32_000 &&
    typeof image.size === 'string' &&
    ['1024x1024', '1536x1024', '1024x1536'].includes(image.size) &&
    typeof image.quality === 'string' &&
    ['low', 'medium', 'high'].includes(image.quality) &&
    typeof image.background === 'string' &&
    ['opaque', 'transparent'].includes(image.background) &&
    typeof image.definition_version === 'string' &&
    /^[a-zA-Z0-9_./-]{1,128}$/.test(image.definition_version) &&
    hash(image.price_digest) &&
    typeof image.max_image_bytes === 'string' &&
    /^[0-9]{1,8}$/.test(image.max_image_bytes) &&
    BigInt(image.max_image_bytes) > 0n &&
    BigInt(image.max_image_bytes) <= 12n * 1024n * 1024n &&
    typeof image.source_id === 'string' &&
    image.source_id.length > 0 &&
    hash(image.source_sha256)
  );
}

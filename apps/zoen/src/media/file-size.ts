/** Shared by received files and staged attachments; small selections stay legible. */
export function formatFileSize(bytes: string | number, locale: string): string {
  const value = Number(bytes);
  if (value < 1024) return `${value} B`;
  const unit = value < 1048576 ? 'KB' : 'MB';
  const divisor = value < 1048576 ? 1024 : 1048576;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value / divisor)} ${unit}`;
}

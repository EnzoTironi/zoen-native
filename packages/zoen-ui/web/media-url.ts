/** Each consumer owns one decrypted URL, released even if opening finishes late. */
export function acquireUrl(open: () => Promise<Blob>, mime: (type: string) => string) {
  let released = false;
  let current: string | undefined;
  const url = open().then((blob) => {
    const value = URL.createObjectURL(blob.slice(0, blob.size, mime(blob.type)));
    if (released) URL.revokeObjectURL(value);
    else current = value;
    return value;
  });
  return { url, release() {
    released = true;
    if (current) { URL.revokeObjectURL(current); current = undefined; }
  } };
}

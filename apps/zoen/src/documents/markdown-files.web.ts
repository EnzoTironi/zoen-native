import { t } from '../i18n/index.ts';
import * as DocumentPicker from 'expo-document-picker';
import { markdownFilename, validateMarkdown } from './editing';

export async function pickMarkdown(): Promise<{ path: string; markdown: string } | null> {
  const result = await DocumentPicker.getDocumentAsync({
    type: ['text/markdown', 'text/plain', 'text/x-markdown'],
    base64: false,
  });
  if (result.canceled) return null;
  const asset = result.assets[0];
  if (!asset?.file) throw new Error(t('documents.theBrowserDidNotMakeThisFileAvailable'));
  if (asset.file.size > 768 * 1024) throw new Error(t('documents.chooseAMarkdownPageUpTo768KB'));
  return { path: asset.name, markdown: validateMarkdown(await asset.file.text()) };
}

export async function exportMarkdown(title: string, markdown: string): Promise<void> {
  const url = URL.createObjectURL(new Blob([markdown], { type: 'text/markdown;charset=utf-8' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = markdownFilename(title);
  document.body.append(link);
  link.click();
  link.remove();
  // The browser must get a chance to consume the URL before it is revoked.
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

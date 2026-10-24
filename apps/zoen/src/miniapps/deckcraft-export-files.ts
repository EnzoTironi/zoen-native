import { binaryExportFileType } from './binary-formats';
import * as Sharing from 'expo-sharing';
import { Directory, File, FileMode, Paths } from 'expo-file-system';
import * as Crypto from 'expo-crypto';
import { sensitiveCacheWork } from '../media/cache-work';
import { t } from '../i18n/index.ts';
import {
  streamDeckcraftExport,
  type DeckcraftExportOptions,
  type DeckcraftExportFile,
} from './deckcraft-export';

export function shareDeckcraftExport(
  options: Omit<DeckcraftExportOptions, 'create'>,
): Promise<void> {
  return sensitiveCacheWork.run(async () => {
    options.check();
    if (!(await Sharing.isAvailableAsync()))
      throw Error(t('documents.thisDeviceDoesNotSupportFileSharing'));
    options.check();
    await streamDeckcraftExport({
      ...options,
      create(filename, format): DeckcraftExportFile {
        options.check();
        const directory = new Directory(Paths.cache, 'zoen-exports', Crypto.randomUUID());
        directory.create({ intermediates: true, idempotent: false });
        try {
          const file = new File(directory, filename);
          file.create();
          const handle = file.open(FileMode.WriteOnly);
          let closed = false;
          const close = () => {
            if (!closed) {
              closed = true;
              handle.close();
            }
          };
          return {
            write: (bytes) => handle.writeBytes(bytes),
            finish: () => {
              close();
              return file.size;
            },
            share: () =>
              Sharing.shareAsync(file.uri, {
                ...binaryExportFileType(format),
                dialogTitle:
                  format === 'xlsx' || format === 'docx'
                    ? filename
                    : t('decks.shareVersion', { version: options.head.version }),
              }),
            dispose: () => {
              try {
                close();
              } finally {
                if (directory.exists) directory.delete();
              }
            },
          };
        } catch (error) {
          if (directory.exists) directory.delete();
          throw error;
        }
      },
    });
  });
}

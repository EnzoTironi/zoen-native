import type { CoreDriver } from '../core/types';
import { CanonicalBinaryHost } from './canonical-binary.ts';
import type { BinaryEditorId } from './binary-formats.ts';

/** Compose the standard MCP tool adapter with the existing native Rust command
 * boundary. The view supplies its captured frame/account/access lifetime check. */
export function createNativeCanonicalBinaryHost(options: {
  editor?: BinaryEditorId;
  itemId: string;
  driver: Pick<CoreDriver, 'request'>;
  mutate: CoreDriver['request'];
  check: () => void;
  id: () => string;
}) {
  return new CanonicalBinaryHost({
    editor: options.editor,
    itemId: options.itemId,
    id: options.id,
    check: options.check,
    read: async (version, sha256) => {
      options.check();
      const document = await options.driver.request({
        op: 'app_binary_read',
        item_id: options.itemId,
        version: version ?? null,
        expected_sha256: sha256 ?? null,
      });
      options.check();
      return document;
    },
    save: async (input) => {
      options.check();
      const document = await options.mutate({ op: 'app_binary_save', ...input });
      options.check();
      return document;
    },
  });
}

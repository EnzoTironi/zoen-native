import type { DeckcraftExportHead } from './deckcraft-export';
import type { AppSpecDto } from '../core/generated/AppSpecDto';
import type { ItemDetail } from '../core/types';
import { AppKeyboardViewport } from './app-keyboard-viewport';
import { CanonicalDrawingView } from './canonical-view';
import { DeckcraftView } from './deckcraft-view';
import type { MiniappDocumentController } from './document-controller';
import { BuiltinMiniappView } from './view-host-builtin';
import { isBinaryEditorId } from './binary-formats';

export function MiniappView({
  selectedHead,
  onDocumentController,
  onDocumentCopied,
  onDocumentDiscard,
  ...props
}: {
  selectedHead?: DeckcraftExportHead;
  item: ItemDetail;
  spec: AppSpecDto;
  fill?: boolean;
  onDocumentController?: (controller: MiniappDocumentController | null) => void;
  onDocumentCopied?: (itemId: string) => void;
  onDocumentDiscard?: () => void;
}) {
  const content = isBinaryEditorId(props.spec.id) ? (
    <DeckcraftView
      {...props}
      selectedHead={selectedHead}
      onDocumentController={onDocumentController}
      onDocumentDiscard={onDocumentDiscard}
    />
  ) : props.spec.id === 'excalidraw' ? (
    <CanonicalDrawingView
      {...props}
      onDocumentController={onDocumentController}
      onDocumentCopied={onDocumentCopied}
      onDocumentDiscard={onDocumentDiscard}
    />
  ) : (
    <BuiltinMiniappView {...props} />
  );
  return (
    <AppKeyboardViewport
      enabled={!!props.fill}
      scope={JSON.stringify([props.item.id, props.spec.resource_uri])}
    >
      {content}
    </AppKeyboardViewport>
  );
}

import type { CSSProperties } from 'react';
import { useAnchoredMenu } from '../components/use-anchored-menu.web';
import { Icon } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps } from './media-actions.types';

const styles = `
.zoen-attachment-trigger, .zoen-attachment-item {
  font: inherit; color: inherit; cursor: pointer; -webkit-tap-highlight-color: transparent;
  font-family: -apple-system, system-ui, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  border: 1px solid transparent;
}
.zoen-attachment-trigger {
  display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  width: 52px; height: 52px; padding: 0; border-radius: 50%;
  background: var(--attachment-surface);
}
.zoen-attachment-trigger:disabled { cursor: default; opacity: .4; }
.zoen-attachment-menu {
  position: fixed; inset: auto; margin: 0; box-sizing: border-box;
  width: 248px; max-width: calc(100vw - 24px); padding: 8px; border-radius: 21px;
  border: 1px solid transparent; background: var(--attachment-raised); color: var(--attachment-text);
  box-shadow: 0 12px 36px #0000001f, 0 2px 8px #00000012, 0 0 0 1px var(--attachment-line);
  overflow: auto; overscroll-behavior: contain; opacity: 0;
  transition: opacity 150ms ease-out, display 150ms allow-discrete, overlay 150ms allow-discrete;
}
.zoen-attachment-menu:popover-open { opacity: 1; }
.zoen-attachment-item {
  display: flex; align-items: center; gap: 12px; width: 100%; min-height: 46px;
  padding: 10px 12px; border-radius: 12px; background: transparent; text-align: start;
  font-size: 15px; font-weight: 500; line-height: 22px;
}
.zoen-attachment-item:focus-visible { background: var(--attachment-muted); }
.zoen-attachment-trigger:focus-visible, .zoen-attachment-item:focus-visible {
  outline: 2px solid var(--attachment-focus); outline-offset: 2px;
}
@media (hover: hover) {
  .zoen-attachment-trigger:not(:disabled):hover, .zoen-attachment-item:hover {
    background: var(--attachment-muted);
  }
}
@media (prefers-reduced-motion: no-preference) {
  .zoen-attachment-trigger { transition: scale 150ms ease-out; }
  .zoen-attachment-trigger:not(:disabled):active { scale: .96; }
  .zoen-attachment-menu { transform: translateY(4px); transition-property: opacity, transform, display, overlay; }
  .zoen-attachment-menu:popover-open { transform: translateY(0); }
  @starting-style { .zoen-attachment-menu:popover-open { opacity: 0; transform: translateY(4px); } }
}
@media (prefers-reduced-motion: reduce) {
  @starting-style { .zoen-attachment-menu:popover-open { opacity: 0; } }
}
`;

export function MediaActions({ disabled, hasVoiceEditor, onChoose }: MediaActionsProps) {
  const { colors, scheme } = useTheme();
  const { trigger, menu, select, triggerProps, menuProps } = useAnchoredMenu({ disabled });
  const variables = {
    '--attachment-surface': colors.surface,
    '--attachment-raised': colors.raised,
    '--attachment-muted': colors.muted,
    '--attachment-text': colors.text,
    '--attachment-line': colors.line,
    '--attachment-focus': colors.text,
    color: colors.text,
    colorScheme: scheme,
  } as CSSProperties;

  return (
    <>
      <style>{styles}</style>
      <button
        ref={trigger}
        type="button"
        data-testid="add-attachment"
        className="zoen-attachment-trigger"
        style={variables}
        aria-label={t('media.addAttachment')}
        disabled={disabled}
        {...triggerProps}
      >
        <Icon name={{ web: 'add', android: 'add', ios: 'plus' }} size={26} />
      </button>
      <div
        ref={menu}
        {...menuProps}
        aria-label={t('media.addAttachment')}
        className="zoen-attachment-menu"
        style={variables}
      >
        {(
          [
            ['photo', t('media.photosAndVideos'), 'photo_library'],
            ['camera', t('media.camera'), 'photo_camera'],
            ...(hasVoiceEditor
              ? [['audio', t('media.voiceImportAudio'), 'graphic_eq'] as const]
              : []),
            ['document', t('media.file2'), 'description'],
          ] as const
        ).map(([kind, label, icon]) => (
          <button
            key={kind}
            type="button"
            role="menuitem"
            aria-label={label}
            tabIndex={-1}
            disabled={disabled}
            className="zoen-attachment-item"
            onClick={() => select(() => onChoose(kind))}
          >
            <span aria-hidden="true">
              <Icon name={{ web: icon, android: icon }} size={22} />
            </span>
            {label}
          </button>
        ))}
      </div>
    </>
  );
}

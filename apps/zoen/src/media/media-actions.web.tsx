import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from 'react';
import { Icon } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps, MediaPickKind } from './media-actions.types';

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
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const openingFocus = useRef<'first' | 'last'>('first');
  const [open, setOpen] = useState(false);
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

  const position = useCallback(() => {
    const button = trigger.current;
    const panel = menu.current;
    if (!button || !panel) return;
    const rect = button.getBoundingClientRect();
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft ?? 0;
    const top = viewport?.offsetTop ?? 0;
    const width = viewport?.width ?? window.innerWidth;
    const height = viewport?.height ?? window.innerHeight;
    const panelWidth = Math.min(248, Math.max(0, width - 24));
    const above = Math.max(0, rect.top - top - 20);
    const below = Math.max(0, top + height - rect.bottom - 20);
    const alignRight = getComputedStyle(button).direction === 'rtl';
    panel.style.left = `${Math.max(left + 12, Math.min(alignRight ? rect.right - panelWidth : rect.left, left + width - panelWidth - 12))}px`;
    panel.style.width = `${panelWidth}px`;
    const showAbove = above >= below;
    panel.style.top = showAbove ? 'auto' : `${rect.bottom + 8}px`;
    panel.style.bottom = showAbove ? `${window.innerHeight - rect.top + 8}px` : 'auto';
    panel.style.maxHeight = `${showAbove ? above : below}px`;
    panel.style.transformOrigin = `${alignRight ? 'right' : 'left'} ${showAbove ? 'bottom' : 'top'}`;
  }, []);

  useEffect(() => {
    if (!open) return;
    // The visual viewport also changes when the software keyboard opens or closes.
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    const viewport = window.visualViewport;
    viewport?.addEventListener('resize', position);
    viewport?.addEventListener('scroll', position);
    return () => {
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
      viewport?.removeEventListener('resize', position);
      viewport?.removeEventListener('scroll', position);
    };
  }, [open, position]);
  useEffect(() => {
    if (disabled) menu.current?.hidePopover();
  }, [disabled]);

  function items() {
    return Array.from(menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? []);
  }
  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    openingFocus.current = event.key === 'ArrowUp' ? 'last' : 'first';
    menu.current?.showPopover({ source: event.currentTarget });
    const buttons = items();
    (openingFocus.current === 'last' ? buttons.at(-1) : buttons[0])?.focus();
  }
  function onMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Tab') {
      // Return to the invoker before the browser advances focus; this is not a focus trap.
      menu.current?.hidePopover();
      trigger.current?.focus();
      return;
    }
    const buttons = items();
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    let next: HTMLButtonElement | undefined;
    if (event.key === 'ArrowDown') next = buttons[(index + 1) % buttons.length];
    else if (event.key === 'ArrowUp') next = buttons[(index - 1 + buttons.length) % buttons.length];
    else if (event.key === 'Home') next = buttons[0];
    else if (event.key === 'End') next = buttons.at(-1);
    else if (event.key.length === 1 && !event.altKey && !event.ctrlKey && !event.metaKey) {
      const ordered = [...buttons.slice(index + 1), ...buttons.slice(0, index + 1)];
      next = ordered.find((button) =>
        button
          .getAttribute('aria-label')
          ?.toLocaleLowerCase()
          .startsWith(event.key.toLocaleLowerCase()),
      );
    }
    if (next) {
      event.preventDefault();
      next.focus();
    }
  }
  function choose(kind: MediaPickKind) {
    if (disabled) return;
    menu.current?.hidePopover();
    trigger.current?.focus();
    // File pickers require the original user activation: never defer this callback.
    onChoose(kind);
  }

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
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={id}
        disabled={disabled}
        popoverTarget={id}
        onKeyDown={onTriggerKeyDown}
      >
        <Icon name={{ web: 'add', android: 'add', ios: 'plus' }} size={26} />
      </button>
      <div
        ref={menu}
        id={id}
        popover="auto"
        role="menu"
        tabIndex={-1}
        aria-label={t('media.addAttachment')}
        className="zoen-attachment-menu"
        style={variables}
        onBeforeToggle={(event) => {
          if (event.newState === 'open') position();
        }}
        onToggle={(event) => {
          const visible = event.newState === 'open';
          setOpen(visible);
          if (visible) {
            const buttons = items();
            (openingFocus.current === 'last' ? buttons.at(-1) : buttons[0])?.focus();
          } else openingFocus.current = 'first';
        }}
        onKeyDown={onMenuKeyDown}
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
            onClick={() => choose(kind)}
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

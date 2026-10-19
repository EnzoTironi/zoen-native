import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';

type AnchoredMenuOptions = {
  disabled?: boolean;
  width?: number;
  align?: 'start' | 'end';
};

/** Browser-owned nonmodal popover; selection stays in the original user activation. */
export function useAnchoredMenu({
  disabled = false,
  width: preferredWidth = 248,
  align = 'start',
}: AnchoredMenuOptions = {}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const openingFocus = useRef<'first' | 'last'>('first');
  const [open, setOpen] = useState(false);
  // Dropping the last Home action can remove the native popover before a toggle event.
  if (disabled && open) setOpen(false);
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
    const panelWidth = Math.min(preferredWidth, Math.max(0, width - 24));
    const above = Math.max(0, rect.top - top - 20);
    const below = Math.max(0, top + height - rect.bottom - 20);
    const alignRight = (getComputedStyle(button).direction === 'rtl') !== (align === 'end');
    panel.style.left = `${Math.max(left + 12, Math.min(alignRight ? rect.right - panelWidth : rect.left, left + width - panelWidth - 12))}px`;
    panel.style.width = `${panelWidth}px`;
    const showAbove = above >= below;
    panel.style.top = showAbove ? 'auto' : `${rect.bottom + 8}px`;
    panel.style.bottom = showAbove ? `${window.innerHeight - rect.top + 8}px` : 'auto';
    panel.style.maxHeight = `${showAbove ? above : below}px`;
    panel.style.transformOrigin = `${alignRight ? 'right' : 'left'} ${showAbove ? 'bottom' : 'top'}`;
  }, [align, preferredWidth]);

  useEffect(() => {
    if (!open || disabled) return;
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
  }, [disabled, open, position]);
  useEffect(() => {
    if (disabled) menu.current?.hidePopover();
  }, [disabled]);

  function items() {
    return Array.from(
      menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [],
    );
  }
  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (disabled || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return;
    event.preventDefault();
    openingFocus.current = event.key === 'ArrowUp' ? 'last' : 'first';
    menu.current?.showPopover({ source: event.currentTarget });
    const buttons = items();
    (openingFocus.current === 'last' ? buttons.at(-1) : buttons[0])?.focus();
  }
  function onMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Tab') {
      // Restore the invoker before the browser advances focus; never trap Tab.
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
  function select(onChoose: () => void) {
    // Native closure is synchronous, unlike the queued React toggle event.
    if (disabled || !menu.current?.matches(':popover-open')) return;
    menu.current.hidePopover();
    trigger.current?.focus();
    // In particular, do not defer native file-picker callbacks to an effect or timer.
    onChoose();
  }
  return {
    trigger,
    menu,
    select,
    triggerProps: {
      'aria-haspopup': 'menu' as const,
      'aria-expanded': open && !disabled,
      'aria-controls': id,
      popoverTarget: id,
      onKeyDown: onTriggerKeyDown,
    },
    menuProps: {
      id,
      popover: 'auto' as const,
      role: 'menu',
      tabIndex: -1,
      onBeforeToggle: (event: { newState: string }) => {
        if (event.newState === 'open') position();
      },
      onToggle: (event: { newState: string }) => {
        const visible = event.newState === 'open';
        setOpen(visible);
        if (visible) {
          const buttons = items();
          (openingFocus.current === 'last' ? buttons.at(-1) : buttons[0])?.focus();
        } else openingFocus.current = 'first';
      },
      onKeyDown: onMenuKeyDown,
    },
  };
}

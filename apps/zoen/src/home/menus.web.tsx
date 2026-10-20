import { View } from 'react-native';
import { router } from 'expo-router';
import type { CSSProperties } from 'react';
import { Icon, NativeButton } from '../components/primitives';
import { useAnchoredMenu } from '../components/use-anchored-menu.web';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { homeCardActions, type HomeCardActionId } from './card-actions';
import type { HomeActionsProps, HomeCardMenuProps } from './menus.types';

const styles = `
.zoen-card-menu-trigger, .zoen-card-menu-item {
  font: inherit; font-family: -apple-system, system-ui, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  color: inherit; cursor: pointer; -webkit-tap-highlight-color: transparent;
}
.zoen-card-menu-trigger {
  display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  width: 44px; height: 44px; padding: 0; border-radius: 50%;
  border: 1px solid var(--card-menu-line); background: var(--card-menu-surface);
}
.zoen-card-menu-panel {
  position: fixed; inset: auto; margin: 0; box-sizing: border-box;
  width: 248px; max-width: calc(100vw - 24px); padding: 8px; border-radius: 21px;
  border: 1px solid transparent; background: var(--card-menu-raised); color: var(--card-menu-text);
  box-shadow: 0 12px 36px #0000001f, 0 2px 8px #00000012, 0 0 0 1px var(--card-menu-line);
  overflow: auto; overscroll-behavior: contain; opacity: 0;
  transition: opacity 150ms ease-out, display 150ms allow-discrete, overlay 150ms allow-discrete;
}
.zoen-card-menu-panel:popover-open { opacity: 1; }
.zoen-card-menu-item {
  display: flex; align-items: center; gap: 12px; width: 100%; min-height: 46px;
  padding: 10px 12px; border: 1px solid transparent; border-radius: 12px;
  background: transparent; text-align: start; font-size: 15px; font-weight: 500; line-height: 22px;
  overflow-wrap: anywhere;
}
.zoen-card-menu-item:focus-visible { background: var(--card-menu-muted); }
.zoen-card-menu-trigger:focus-visible, .zoen-card-menu-item:focus-visible {
  outline: 2px solid var(--card-menu-text); outline-offset: 2px;
}
@media (hover: hover) {
  .zoen-card-menu-trigger:hover, .zoen-card-menu-item:hover { background: var(--card-menu-muted); }
}
@media (prefers-reduced-motion: no-preference) {
  .zoen-card-menu-trigger { transition: scale 150ms ease-out; }
  .zoen-card-menu-trigger:active { scale: .96; }
  .zoen-card-menu-panel { transform: translateY(4px); transition-property: opacity, transform, display, overlay; }
  .zoen-card-menu-panel:popover-open { transform: translateY(0); }
  @starting-style { .zoen-card-menu-panel:popover-open { opacity: 0; transform: translateY(4px); } }
}
@media (prefers-reduced-motion: reduce) {
  @starting-style { .zoen-card-menu-panel:popover-open { opacity: 0; } }
}
`;
const icons = {
  'home-pin': 'push_pin',
  'conversation-pin': 'visibility',
  'conversation-organize': 'tune',
  'move-earlier': 'arrow_upward',
  'move-later': 'arrow_downward',
  history: 'history',
  details: 'info',
} as const satisfies Record<HomeCardActionId, string>;

// Preserve the current generic HomeActions behavior; wider action parity is separate.
export function HomeActions({ hiddenCount = 0, onRestoreHidden }: HomeActionsProps) {
  return (
    <View style={{ gap: 8, alignItems: 'flex-end' }}>
      <NativeButton label={t('home.addMiniapp')} onPress={() => router.push('/miniapps')} />
      {hiddenCount > 0 && onRestoreHidden ? (
        <NativeButton
          testID="home-restore-hidden-apps"
          label={t('home.restoreHiddenCount', { count: hiddenCount })}
          secondary
          onPress={onRestoreHidden}
        />
      ) : null}
    </View>
  );
}

export function HomeCardMenu(props: HomeCardMenuProps) {
  const actions = homeCardActions(props);
  const { colors, scheme } = useTheme();
  const { trigger, menu, select, triggerProps, menuProps } = useAnchoredMenu({
    disabled: actions.length === 0,
    align: 'end',
  });
  if (!actions.length) return null;
  const label = t('home.options', { name: props.name });
  const variables = {
    '--card-menu-surface': colors.surface,
    '--card-menu-raised': colors.raised,
    '--card-menu-muted': colors.muted,
    '--card-menu-text': colors.text,
    '--card-menu-line': colors.line,
    color: colors.text,
    colorScheme: scheme,
  } as CSSProperties;
  return (
    <>
      <style>{styles}</style>
      <button
        ref={trigger}
        type="button"
        data-testid={props.testID}
        className="zoen-card-menu-trigger"
        style={variables}
        aria-label={label}
        {...triggerProps}
      >
        <span aria-hidden="true">
          <Icon name={{ web: 'more_horiz', android: 'more_horiz', ios: 'ellipsis' }} size={22} />
        </span>
      </button>
      <div
        ref={menu}
        {...menuProps}
        aria-label={label}
        className="zoen-card-menu-panel"
        style={variables}
      >
        {actions.map((action) => (
          <button
            key={action.id}
            type="button"
            role="menuitem"
            data-action={action.id}
            aria-label={action.label}
            tabIndex={-1}
            className="zoen-card-menu-item"
            onClick={() => select(action.onPress)}
          >
            <span aria-hidden="true">
              <Icon name={{ web: icons[action.id], android: icons[action.id] }} size={22} />
            </span>
            {action.label}
          </button>
        ))}
      </div>
    </>
  );
}

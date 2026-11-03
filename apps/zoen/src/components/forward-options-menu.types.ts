import type { ForwardOptions } from '../core/generated/ForwardOptions';

export type ForwardOptionsMenuProps = {
  options: Readonly<ForwardOptions>;
  canHideCaption: boolean;
  disabled?: boolean;
  remaining?: boolean;
  onChange(options: ForwardOptions): void;
};

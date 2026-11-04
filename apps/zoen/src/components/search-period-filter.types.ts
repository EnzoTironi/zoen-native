import type { SearchPeriod } from '../core/generated/SearchPeriod';
export type SearchPeriodFilterProps = {
  value?: SearchPeriod;
  selected: boolean;
  isCurrent(): boolean;
  onChange(value: SearchPeriod): void;
};

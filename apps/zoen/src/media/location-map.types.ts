import type { StaticLocation } from './types';
export type LocationMapProps = {
  binding: string;
  editable: boolean;
  point?: StaticLocation;
  label: string;
  onPointChange?(point: StaticLocation): void;
};

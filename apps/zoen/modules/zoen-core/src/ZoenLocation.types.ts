/** Transient, exact GeoJSON Point projection. MapKit never owns draft/send state. */
export type StaticLocation = Readonly<{
  longitude: number;
  latitude: number;
  label?: string;
}>;

export type LocationMapConfiguration = Readonly<{
  binding: string;
  editable: boolean;
  point?: StaticLocation;
  accessibilityLabel?: string;
}>;

export type LocationPointEvent = Readonly<{
  nativeEvent: Readonly<{ binding: string; point: StaticLocation }>;
}>;

import { Host } from '@expo/ui';
import { ColorPicker } from '@expo/ui/swift-ui';
import { accessibilityLabel, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';

export function ThemeColorPicker({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange(value: string): void;
}) {
  return (
    <Host matchContents style={{ width: 44, height: 44 }}>
      <ColorPicker
        label={label}
        selection={value}
        supportsOpacity={false}
        onSelectionChange={(color) => {
          const opaque = /^#[\da-f]{8}$/i.test(color) ? color.slice(0, 7) : color;
          if (/^#[\da-f]{6}$/i.test(opaque)) onChange(opaque.toUpperCase());
        }}
        modifiers={[
          labelStyle('iconOnly'),
          accessibilityLabel(label),
          frame({ width: 44, height: 44 }),
        ]}
      />
    </Host>
  );
}

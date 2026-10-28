/** Minimal dependency-aware hook host for the real bundled hook and route. */
export function hookHost() {
  const slots: Array<{ value?: unknown; deps?: readonly unknown[]; cleanup?: () => void }> = [];
  let cursor = 0;
  let dirty = false;
  let focused = true;
  const effects: Array<() => void> = [];
  const focusSlots = new Set<number>();
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[] | undefined) =>
    a !== undefined &&
    b !== undefined &&
    a.length === b.length &&
    a.every((value, index) => Object.is(value, b[index]));
  const memo = (create: () => unknown, deps?: readonly unknown[]) => {
    const index = cursor++;
    const slot = (slots[index] ??= {});
    if (!same(slot.deps, deps)) {
      slot.value = create();
      slot.deps = deps;
    }
    return slot.value;
  };
  const effect = (create: () => void | (() => void), deps?: readonly unknown[]) => {
    const index = cursor++;
    const slot = (slots[index] ??= {});
    if (!same(slot.deps, deps)) {
      slot.deps = deps;
      effects.push(() => {
        slot.cleanup?.();
        slot.cleanup = create() || undefined;
      });
    }
  };
  const react = {
    useRef: (value: unknown) => memo(() => ({ current: value }), []),
    useMemo: memo,
    useCallback: (value: unknown, deps?: readonly unknown[]) => memo(() => value, deps),
    useState: (initial: unknown) => {
      const index = cursor++;
      const slot = (slots[index] ??= { value: initial });
      return [
        slot.value,
        (next: unknown) => {
          const value = typeof next === 'function' ? next(slot.value) : next;
          if (!Object.is(value, slot.value)) {
            slot.value = value;
            dirty = true;
          }
        },
      ];
    },
    useEffect: effect,
    useLayoutEffect: effect,
  };
  return {
    react,
    useFocusEffect: (create: () => void | (() => void)) => {
      const index = cursor;
      focusSlots.add(index);
      effect(() => (focused ? create() : undefined), [create, focused]);
    },
    render<T>(component: () => T): T {
      let value!: T;
      for (let attempts = 0; attempts < 20; attempts++) {
        dirty = false;
        cursor = 0;
        value = component();
        for (const work of effects.splice(0)) work();
        if (!dirty) return value;
      }
      throw new Error('Hook host did not settle');
    },
    blur: () => {
      focused = false;
      for (const index of focusSlots) {
        slots[index]?.cleanup?.();
        if (slots[index]) slots[index]!.cleanup = undefined;
      }
    },
  };
}

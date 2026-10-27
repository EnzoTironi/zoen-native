import * as React from 'react';

type Control = Record<string, any>;

/** Capture only controls belonging to a visible compiled sheet/screen. */
export function editorAgentControlPorts(controls: Map<string, Control>) {
  const visibility = React.createContext(true);
  const children = ({ children }: { children: React.ReactNode }) => children;
  function VisibleView(props: Control) {
    const parent = React.useContext(visibility);
    const hidden = [props.style].flat().some((style) => style?.display === 'none');
    return React.createElement(visibility.Provider, { value: parent && !hidden }, props.children);
  }
  function Capture(props: Control) {
    const visible = React.useContext(visibility);
    const key = visible
      ? (props.testID ?? (props.label ? `label:${props.label}` : undefined))
      : undefined;
    React.useLayoutEffect(() => {
      if (key) controls.set(key, props);
      return () => {
        if (key && controls.get(key) === props) controls.delete(key);
      };
    }, [key, props]);
    return null;
  }
  return { children, VisibleView, Capture };
}

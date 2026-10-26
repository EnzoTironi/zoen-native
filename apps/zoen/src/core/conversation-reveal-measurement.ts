import type { LayoutChangeEvent, TextLayoutEvent } from 'react-native';
import type { ConversationContentLayout } from './conversation-reveal-content';

type Node = Exclude<LayoutChangeEvent['currentTarget'], number>;
type Frame = Readonly<{ x: number; y: number; width: number; height: number }>;
type Line = TextLayoutEvent['nativeEvent']['lines'][number];
const usable = (frame: Frame) =>
  [frame.x, frame.y, frame.width, frame.height].every(Number.isFinite) &&
  frame.width > 0 &&
  frame.height > 0;

/** A semantic text anchor, measured relative to its own row content rather than
 * the window. Native scroll/keyboard movement therefore does not change it. */
export class ConversationRevealMeasurement {
  private active = true;
  private revision = 0;
  private bodyNode: Node | null = null;
  private bodyFrame: Frame | null = null;
  private anchorNode: Node | null = null;
  private anchorFrame: Frame | null = null;
  private lines: readonly Line[] = [];
  private lineNode: Node | null = null;
  private query = '';
  private last = '';
  private readonly expectsText: boolean;
  private readonly report: (layout: ConversationContentLayout) => void;
  constructor(expectsText: boolean, report: (layout: ConversationContentLayout) => void) {
    this.expectsText = expectsText;
    this.report = report;
  }
  setQuery(query: string) {
    if (this.query === query) return;
    this.query = query;
    ++this.revision;
    this.measure();
  }
  activate() {
    this.active = true;
    ++this.revision;
    this.measure();
  }
  retire() {
    this.active = false;
    ++this.revision;
  }
  body = (event: LayoutChangeEvent) => {
    if (!this.active || !usable(event.nativeEvent.layout)) return;
    const node = this.node(event.currentTarget);
    if (node === this.bodyNode && this.sameFrame(this.bodyFrame, event.nativeEvent.layout)) return;
    if (
      this.bodyFrame &&
      (this.bodyFrame.width !== event.nativeEvent.layout.width ||
        this.bodyFrame.height !== event.nativeEvent.layout.height)
    )
      this.lines = [];
    this.bodyFrame = { ...event.nativeEvent.layout };
    this.bodyNode = node;
    ++this.revision;
    this.emit({ ...this.bodyFrame, anchorPending: Boolean(this.expectsText && this.bodyNode) });
    this.measure();
  };
  anchor = (event: LayoutChangeEvent) => {
    if (!this.active || !usable(event.nativeEvent.layout)) return;
    const node = this.node(event.currentTarget);
    if (node === this.anchorNode && this.sameFrame(this.anchorFrame, event.nativeEvent.layout))
      return;
    if (node !== this.lineNode) this.lines = [];
    this.anchorNode = node;
    this.anchorFrame = { ...event.nativeEvent.layout };
    ++this.revision;
    if (!node && this.bodyFrame) this.emit({ ...this.bodyFrame });
    this.measure();
  };
  text = (event: TextLayoutEvent) => {
    if (!this.active) return;
    const node = this.node(event.currentTarget);
    if (node && this.anchorNode && node !== this.anchorNode) return;
    if (JSON.stringify(this.lines) === JSON.stringify(event.nativeEvent.lines)) return;
    this.lines = event.nativeEvent.lines;
    this.lineNode = node;
    ++this.revision;
    this.measure();
  };
  private node(node: LayoutChangeEvent['currentTarget']): Node | null {
    return typeof node === 'object' && node && typeof node.measureLayout === 'function'
      ? node
      : null;
  }
  private sameFrame(a: Frame | null, b: Frame) {
    return a?.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
  }
  private selectedLine(): Frame | null {
    const query = this.query.trim().toLowerCase();
    if (!query || !this.lines.length) return null;
    const text = this.lines
      .map((line) => line.text)
      .join('')
      .toLowerCase();
    const index = text.indexOf(query);
    if (index < 0) return null;
    let start = 0;
    for (const line of this.lines) {
      const end = start + line.text.toLowerCase().length;
      if (index < end && usable(line) && line.x >= 0 && line.y >= 0) return line;
      start = end;
    }
    return null;
  }
  private measure() {
    const { bodyNode, bodyFrame, anchorNode, anchorFrame } = this;
    if (!this.active || !bodyFrame || !bodyNode || !anchorNode || !anchorFrame) return;
    const revision = this.revision;
    anchorNode.measureLayout(
      bodyNode,
      (x, y, width, height) => {
        if (!this.active || revision !== this.revision) return;
        const frame = { x, y, width, height };
        if (
          !usable(frame) ||
          x < 0 ||
          y < 0 ||
          x + width > bodyFrame.width + 0.5 ||
          y + height > bodyFrame.height + 0.5 ||
          Math.abs(width - anchorFrame.width) > 0.5 ||
          Math.abs(height - anchorFrame.height) > 0.5
        )
          return;
        const line = this.selectedLine();
        const anchor =
          line && line.y + line.height <= height + 0.5 && line.x + line.width <= width + 0.5
            ? { y: y + line.y, width: line.width, height: line.height }
            : { y, width, height };
        this.emit({ ...bodyFrame, anchor, anchorPending: false });
      },
      () => {
        if (this.active && revision === this.revision) this.emit({ ...bodyFrame });
      },
    );
  }
  private emit(layout: ConversationContentLayout) {
    const key = JSON.stringify(layout);
    if (!this.active || key === this.last) return;
    this.last = key;
    this.report(layout);
  }
}

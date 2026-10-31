export type CalendarNavigationProps = {
  spaceId: string;
  onOpen(): void;
  reveal(id: string): Promise<unknown>;
  onSelect(id: string): void;
};

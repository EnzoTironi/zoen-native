export type MiniappFrameHandle = { send: (message: unknown) => void; dispose: () => void };
export type MiniappFrameProps = {
  html: string;
  onMessage: (message: string) => void;
  onError: (message: string) => void;
  height: number;
  title: string;
  blocked: boolean;
  fill?: boolean;
};

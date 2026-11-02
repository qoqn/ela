export type PaintTimes = { received: number; decoded: number; painting: number; drawn: number };
export type PieceRect = { x: number; y: number; w: number; h: number };

export interface FrameProbe {
  sent(): void;
  painted(times: PaintTimes, rects: PieceRect[]): void;
  animationFrame(): void;
}

export interface TerminalProbe {
  received(bytes: Uint8Array): void;
  written(): void;
  echoParsed(data: string): void;
  rendered(): void;
}

export type Drive = {
  pointer?: (step: number) => { x: number; y: number };
  key?: (down: boolean) => void;
  mark?: () => number;
  type?: (data: string) => void;
};

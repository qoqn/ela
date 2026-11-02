import type { RemoteCursor } from '../../ipc';

// A larger image is ignored or could cover the app's own frame (design: Mouse).
const MAX_CSS_SIZE = 128;

type Image = Extract<RemoteCursor, { shape: 'image' }>;

export function cursorPlacement(cursor: Image, viewScale: number, dpr: number) {
  const longest = Math.max(cursor.width, cursor.height);
  const scale = Math.min(viewScale > 0 ? viewScale : 1, MAX_CSS_SIZE / longest);
  return {
    pixelWidth: Math.max(1, Math.round(cursor.width * scale * dpr)),
    pixelHeight: Math.max(1, Math.round(cursor.height * scale * dpr)),
    hotX: Math.round(cursor.hot_x * scale),
    hotY: Math.round(cursor.hot_y * scale),
  };
}

function imageSet(url: string, dpr: number) {
  if (dpr === 1) return `url(${url})`;
  const set = CSS.supports('cursor', 'image-set(url(data:,) 1x) 0 0, auto')
    ? 'image-set'
    : '-webkit-image-set';
  return `${set}(url(${url}) ${dpr}x)`;
}

export function cursorStyle(cursor: RemoteCursor, viewScale: number, dpr: number): string {
  if (cursor.shape === 'default') return 'auto';
  if (cursor.shape === 'hidden') return 'none';
  const { pixelWidth, pixelHeight, hotX, hotY } = cursorPlacement(cursor, viewScale, dpr);
  const source = document.createElement('canvas');
  source.width = cursor.width;
  source.height = cursor.height;
  const rgba = new Uint8ClampedArray(cursor.rgba);
  source.getContext('2d')!.putImageData(new ImageData(rgba, cursor.width, cursor.height), 0, 0);
  const sized = document.createElement('canvas');
  sized.width = pixelWidth;
  sized.height = pixelHeight;
  sized.getContext('2d')!.drawImage(source, 0, 0, pixelWidth, pixelHeight);
  return `${imageSet(sized.toDataURL('image/png'), dpr)} ${hotX} ${hotY}, auto`;
}

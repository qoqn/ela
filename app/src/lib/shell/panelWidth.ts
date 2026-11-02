export type Section = 'connections' | 'files' | 'vault' | 'knownhosts';
export type PanelWidths = Partial<Record<Section, number>>;

export const PANEL_MIN = 200;
export const PANEL_MAX = 480;

// Files and Known Hosts show paths and fingerprints, which need the room.
export const PANEL_DEFAULT: Record<Section, number> = {
  connections: 264,
  files: 320,
  vault: 264,
  knownhosts: 320,
};

export const clampPanelWidth = (width: number) =>
  Math.round(Math.min(PANEL_MAX, Math.max(PANEL_MIN, width)));

export function readPanelWidths(text: string | null): PanelWidths {
  try {
    const parsed: unknown = JSON.parse(text ?? '{}');
    if (typeof parsed !== 'object' || parsed === null) return {};
    return Object.fromEntries(
      Object.entries(parsed).flatMap(([section, width]) =>
        section in PANEL_DEFAULT && typeof width === 'number'
          ? [[section, clampPanelWidth(width)]]
          : [],
      ),
    );
  } catch {
    return {};
  }
}

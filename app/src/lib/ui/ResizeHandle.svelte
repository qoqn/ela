<script lang="ts">
  type Props = {
    label: string;
    value: number;
    min: number;
    max: number;
    fallback: number;
    onchange: (value: number) => void;
  };
  let { label, value, min, max, fallback, onchange }: Props = $props();

  const KEY_STEP = 16;
  let dragging = $state(false);
  let startX = 0;
  let startValue = 0;

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
    startX = e.clientX;
    startValue = value;
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (dragging) onchange(startValue + e.clientX - startX);
  }

  function onKeydown(e: KeyboardEvent) {
    const next: Record<string, number> = {
      ArrowLeft: value - KEY_STEP,
      ArrowRight: value + KEY_STEP,
      Home: min,
      End: max,
    };
    if (!(e.key in next)) return;
    e.preventDefault();
    e.stopPropagation();
    onchange(next[e.key]);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  role="separator"
  tabindex="0"
  aria-label={label}
  aria-orientation="vertical"
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  title="Drag to resize, double-click for the default width"
  class="absolute inset-y-0 -right-0.5 z-10 w-1 cursor-col-resize transition-colors duration-(--motion-fast) outline-none hover:bg-signal focus-visible:bg-signal {dragging
    ? 'bg-signal'
    : ''}"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)}
  ondblclick={() => onchange(fallback)}
  onkeydown={onKeydown}
></div>

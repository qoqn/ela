<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Props = HTMLButtonAttributes & {
    label: string;
    active?: boolean;
    size?: 'sm' | 'md';
    children: Snippet;
  };
  let { label, active = false, size = 'md', children, class: cls = '', ...rest }: Props = $props();
</script>

<button
  type="button"
  {...rest}
  aria-label={label}
  title={label}
  aria-pressed={active}
  class="inline-grid shrink-0 place-items-center rounded-sm transition-colors duration-(--motion-fast) disabled:pointer-events-none disabled:opacity-35 {size ===
  'sm'
    ? 'size-6'
    : 'size-7'} {active
    ? 'bg-selected text-label'
    : 'text-dim hover:bg-raised hover:text-text'} {cls}"
>
  {@render children()}
</button>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Props = Omit<HTMLInputAttributes, 'checked' | 'type'> & {
    checked?: boolean;
    children: Snippet;
  };
  let { checked = $bindable(false), children, class: cls = '', ...rest }: Props = $props();
</script>

<label class="inline-flex items-center gap-2 text-sm text-text {cls}">
  <input {...rest} type="checkbox" bind:checked class="size-3.5 accent-signal-strong" />
  {@render children()}
</label>

<script lang="ts">
  import { X } from '@lucide/svelte';

  type Props = {
    label: string;
    pressed?: boolean;
    onclick?: () => void;
    onremove?: () => void;
  };
  let { label, pressed, onclick, onremove }: Props = $props();

  const base = 'inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border px-1.5 text-xs';
</script>

{#if onremove}
  <span class="{base} border-line bg-raised text-text">
    {label}
    <button
      type="button"
      aria-label="Remove {label}"
      class="-mr-0.5 text-dim hover:text-text"
      onclick={onremove}><X size={12} /></button
    >
  </span>
{:else}
  <button
    type="button"
    aria-pressed={pressed}
    class="{base} {pressed
      ? 'border-signal bg-selected text-label'
      : 'border-line text-dim hover:text-text'}"
    {onclick}>{label}</button
  >
{/if}

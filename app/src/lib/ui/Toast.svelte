<script lang="ts">
  import Button from './Button.svelte';

  type Props = {
    message: string;
    detail?: string;
    error?: boolean;
    onundo?: () => void;
    ondismiss: () => void;
  };
  let { message, detail, error = false, onundo, ondismiss }: Props = $props();
</script>

<div
  role={error ? 'alert' : 'status'}
  class="absolute right-4 bottom-10 z-30 flex max-w-[48ch] items-center gap-3 rounded-md border bg-panel px-3 py-2 shadow-2 {error
    ? 'border-danger'
    : 'border-line'}"
>
  <div class="min-w-0">
    <p>{message}</p>
    {#if detail}<p class="truncate text-sm text-dim" title={detail}>{detail}</p>{/if}
  </div>
  {#if onundo}<Button size="sm" onclick={onundo}>Undo</Button>{/if}
  <Button size="sm" variant="ghost" onclick={ondismiss}>Dismiss</Button>
</div>

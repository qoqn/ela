<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Props = Omit<HTMLInputAttributes, 'value'> & {
    value?: string;
    mono?: boolean;
    error?: string;
    leading?: Snippet;
    ref?: HTMLInputElement;
  };
  let {
    value = $bindable(''),
    mono = false,
    error = '',
    leading,
    ref = $bindable(),
    class: cls = '',
    ...rest
  }: Props = $props();
</script>

<div class={cls}>
  <div
    class="flex h-7 items-center gap-2 rounded-sm border bg-chassis px-2 transition-colors duration-(--motion-fast) {error
      ? 'border-danger'
      : 'border-edge focus-within:border-signal'}"
  >
    {#if leading}<span class="text-dim">{@render leading()}</span>{/if}
    <input
      {...rest}
      bind:this={ref}
      bind:value
      aria-invalid={error ? 'true' : undefined}
      spellcheck="false"
      autocomplete="off"
      class="min-w-0 flex-1 bg-transparent outline-none placeholder:text-dim/70 {mono
        ? 'font-mono text-sm'
        : ''}"
    />
  </div>
  {#if error}<p class="mt-1 text-sm text-danger-text" role="alert">{error}</p>{/if}
</div>

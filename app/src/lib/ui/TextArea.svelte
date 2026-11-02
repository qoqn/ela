<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';

  type Props = Omit<HTMLTextareaAttributes, 'value'> & {
    value?: string;
    mono?: boolean;
    error?: string;
  };
  let {
    value = $bindable(''),
    mono = false,
    error = '',
    class: cls = '',
    ...rest
  }: Props = $props();
</script>

<div class={cls}>
  <textarea
    {...rest}
    bind:value
    aria-invalid={error ? 'true' : undefined}
    spellcheck="false"
    autocomplete="off"
    class="block w-full resize-y rounded-sm border bg-chassis px-2 py-1 outline-none transition-colors duration-(--motion-fast) placeholder:text-dim/70 {error
      ? 'border-danger'
      : 'border-edge focus:border-signal'} {mono ? 'font-mono text-sm' : ''}"></textarea>
  {#if error}<p class="mt-1 text-sm text-danger-text" role="alert">{error}</p>{/if}
</div>

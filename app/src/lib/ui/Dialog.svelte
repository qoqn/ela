<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    onclose: () => void;
    width?: string;
    children: Snippet;
    footer?: Snippet;
  };
  let { title, onclose, width = '48ch', children, footer }: Props = $props();

  let box = $state<HTMLElement>();
  const titleId = `dialog-${crypto.randomUUID()}`;

  $effect(() => {
    const previous = document.activeElement as HTMLElement | null;
    (box?.querySelector<HTMLElement>('[data-autofocus]') ?? box)?.focus();
    return () => previous?.focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === 'Tab' && box) {
      const focusable = [
        ...box.querySelectorAll<HTMLElement>('button, input, select, [tabindex="0"]'),
      ].filter(
        (element) =>
          !element.hasAttribute('disabled') && element.tabIndex >= 0 && !element.closest('[inert]'),
      );
      if (!focusable.length) return;
      const [first, last] = [focusable[0], focusable[focusable.length - 1]];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

<!-- Fixed, because a Dialog can be opened from inside a Panel, which is a positioned box. -->
<div class="fixed inset-0 z-40 grid place-items-center bg-screen/60 p-4">
  <div
    bind:this={box}
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    style:max-width={width}
    class="flex max-h-full w-full flex-col rounded-md border border-line bg-panel shadow-2 outline-none"
    onkeydown={onKeydown}
  >
    <h2 id={titleId} class="shrink-0 px-4 pt-4 font-medium text-label">{title}</h2>
    <div class="min-h-0 overflow-y-auto px-4 py-3">{@render children()}</div>
    {#if footer}<div class="flex shrink-0 justify-end gap-2 px-4 pb-4">{@render footer()}</div>{/if}
  </div>
</div>

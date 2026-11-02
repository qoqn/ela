<script lang="ts">
  import { X } from '@lucide/svelte';
  import type { Component } from 'svelte';
  import { shell } from '../../shell/shell.svelte';
  import { IconButton } from '../../ui';
  import KeyboardSection from './KeyboardSection.svelte';
  import { currentSection } from './scrollSpy';
  import type { SettingsSection } from './sections';
  import TerminalSection from './TerminalSection.svelte';
  import VaultSection from './VaultSection.svelte';

  const SECTIONS: { id: SettingsSection; title: string; content: Component }[] = [
    { id: 'terminal', title: 'Terminal', content: TerminalSection },
    { id: 'keyboard', title: 'Keyboard', content: KeyboardSection },
    { id: 'vault', title: 'Vault', content: VaultSection },
  ];

  // The section's top padding, so a jump lands with its heading where the first one rests.
  const HEADING_OFFSET = 24;

  let scroller = $state<HTMLElement>();
  let current = $state(0);
  const headings: HTMLElement[] = [];

  function onScroll() {
    if (!scroller) return;
    const top = scroller.getBoundingClientRect().top;
    current = currentSection(
      headings.map((heading) => heading.getBoundingClientRect().top - top),
      HEADING_OFFSET,
    );
  }

  function jumpTo(index: number) {
    if (!scroller) return;
    const top = scroller.getBoundingClientRect().top;
    scroller.scrollTop += headings[index].getBoundingClientRect().top - top - HEADING_OFFSET;
    headings[index].focus({ preventScroll: true });
    onScroll();
  }

  $effect(() => {
    const previous = document.activeElement as HTMLElement | null;
    headings[0]?.focus({ preventScroll: true });
    return () => previous?.focus();
  });

  $effect(() => {
    const index = SECTIONS.findIndex((section) => section.id === shell.settingsTarget);
    if (index >= 0) jumpTo(index);
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    shell.settingsOpen = false;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="@container absolute inset-0 z-20 flex flex-col bg-chassis"
  aria-label="Settings"
  onkeydown={onKeydown}
>
  <header class="flex h-9 shrink-0 items-center border-b border-line bg-panel pr-1.5 pl-4">
    <h1 class="min-w-0 flex-1 truncate text-sm font-semibold text-text">Settings</h1>
    <IconButton label="Close settings" onclick={() => (shell.settingsOpen = false)}>
      <X size={15} />
    </IconButton>
  </header>
  <div class="flex min-h-0 flex-1">
    <nav class="hidden w-44 shrink-0 p-2 @min-[40rem]:block" aria-label="Settings sections">
      {#each SECTIONS as section, index (section.id)}
        <button
          type="button"
          data-row
          aria-current={index === current ? 'location' : undefined}
          class="flex h-7 w-full items-center rounded-sm px-2 text-left text-sm {index === current
            ? 'bg-selected text-label'
            : 'text-dim hover:bg-raised hover:text-text'}"
          onclick={() => jumpTo(index)}>{section.title}</button
        >
      {/each}
    </nav>
    <div
      bind:this={scroller}
      class="min-w-0 flex-1 overflow-y-auto [container-type:size]"
      onscroll={onScroll}
    >
      <div class="flex min-h-full max-w-[72ch] flex-col px-6">
        {#each SECTIONS as section, index (section.id)}
          <!-- The last one fills the view, so a jump can bring even a short section to the top. -->
          <section
            class="pt-6 pb-4"
            class:last-section={index === SECTIONS.length - 1}
            aria-labelledby="settings-{section.id}"
          >
            <h2
              bind:this={headings[index]}
              id="settings-{section.id}"
              tabindex="-1"
              class="pb-2 text-lg font-semibold text-label outline-none"
            >
              {section.title}
            </h2>
            <section.content />
          </section>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .last-section {
    min-height: 100cqh;
  }
</style>

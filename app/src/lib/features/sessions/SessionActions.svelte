<script lang="ts">
  import {
    Camera,
    ChevronDown,
    ClipboardCheck,
    ClipboardX,
    Ellipsis,
    Keyboard,
    RotateCw,
    Search,
    Unplug,
  } from '@lucide/svelte';
  import type { Component } from 'svelte';
  import { workspace, type SessionTab } from '../../sessions.svelte';
  import { runCommand, shell } from '../../shell/shell.svelte';
  import { hostKey } from '../../shell/shortcuts';
  import { Button, IconButton, Kbd, Menu, type MenuItem } from '../../ui';
  import { saved } from '../tree/saved.svelte';
  import { reconnect } from './reconnect.svelte';

  type Action = { label: string; icon: Component; pressed?: boolean; run: () => void };

  let { tab }: { tab: SessionTab } = $props();

  const SIZES = [
    { mode: 'dynamic', label: 'Dynamic' },
    { mode: 'fit', label: 'Fit' },
    { mode: 'original', label: '1:1' },
  ] as const;

  let bar = $state<HTMLElement>();
  let menu = $state<{ items: MenuItem[]; x: number; y: number } | null>(null);

  const capabilities = $derived(workspace.protocol(tab.protocol)?.capabilities);
  const ended = $derived(tab.status.state === 'closed');
  const framebuffer = $derived(tab.renderer === 'framebuffer');
  const sizes = $derived(SIZES.filter(({ mode }) => mode !== 'dynamic' || capabilities?.resize));

  const actions = $derived.by((): Action[] => {
    if (tab.status.state !== 'connected') return [];
    if (tab.renderer === 'terminal') {
      return [{ label: 'Find', icon: Search, pressed: tab.find.open, run: () => tab.find.show() }];
    }
    if (!framebuffer) return [];
    return [
      { label: 'Send Ctrl+Alt+Del', icon: Keyboard, run: () => tab.sendCtrlAltDel() },
      ...(capabilities?.clipboard
        ? [
            {
              label: `Clipboard sync: ${tab.clipboardSync ? 'on' : 'off'}`,
              icon: tab.clipboardSync ? ClipboardCheck : ClipboardX,
              pressed: tab.clipboardSync,
              run: () => (tab.clipboardSync = !tab.clipboardSync),
            },
          ]
        : []),
      { label: 'Save a screenshot', icon: Camera, run: saveScreenshot },
    ];
  });

  const sizeItems = (): MenuItem[] =>
    sizes.map(({ mode, label }) => ({
      label,
      checked: tab.displayMode === mode,
      run: () => (tab.displayMode = mode),
    }));

  function openMenu(e: MouseEvent, items: MenuItem[]) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { items, x: box.left, y: box.bottom };
  }

  async function saveScreenshot() {
    try {
      saved.showToast({ message: 'Screenshot saved', detail: await tab.saveScreenshot() });
    } catch (e) {
      saved.showToast({ message: "Couldn't save the screenshot", detail: String(e), error: true });
    }
  }

  // The bar hides at a narrow width, so the first button is the first one that shows.
  $effect(() => {
    if (!tab.keyboardReleased) return;
    [...(bar?.querySelectorAll('button') ?? [])].find((button) => button.offsetParent)?.focus();
  });

  // With the keyboard released it is back with Ela, except that the host key and its shortcuts
  // keep working. Nothing from here goes to the remote side.
  function onKeydown(e: KeyboardEvent) {
    if (!tab.keyboardReleased) return;
    if (e.key === 'Escape') {
      tab.keyboardReleased = false;
    } else if (e.code === hostKey.code || hostKey.down) {
      e.preventDefault();
      e.stopPropagation();
      const decision = hostKey.keydown(e.code, e.repeat);
      if (decision.kind === 'command') runCommand(decision.command);
    }
  }

  function onKeyup(e: KeyboardEvent) {
    if (!tab.keyboardReleased || e.code !== hostKey.code) return;
    e.stopPropagation();
    if (hostKey.keyup(e.code).kind === 'tap') tab.keyboardReleased = false;
  }
</script>

<!-- Pressing a button leaves focus where it is, so the keyboard stays with the Session. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={bar}
  class="ml-auto flex shrink-0 items-center gap-0.5"
  onmousedown={(e) => e.preventDefault()}
  onkeydown={onKeydown}
  onkeyup={onKeyup}
>
  {#if tab.keyboardReleased}
    <span class="mr-1.5 hidden items-center gap-1.5 text-xs text-dim @min-[38rem]:flex">
      Keyboard released <Kbd keys={shell.hostKeyName} />
    </span>
  {/if}
  {#if actions.length}
    <div class="hidden items-center gap-0.5 @min-[30rem]:flex">
      {#if framebuffer}
        <Button
          size="sm"
          variant="ghost"
          aria-label="Display size"
          aria-haspopup="menu"
          onclick={(e) => openMenu(e, sizeItems())}
        >
          {SIZES.find(({ mode }) => mode === tab.displayMode)?.label}
          <ChevronDown size={12} />
        </Button>
      {/if}
      {#each actions as action (action.icon)}
        <IconButton size="sm" label={action.label} active={action.pressed} onclick={action.run}>
          <action.icon size={14} />
        </IconButton>
      {/each}
    </div>
    <div class="@min-[30rem]:hidden">
      <IconButton
        size="sm"
        label="Session actions"
        aria-haspopup="menu"
        onclick={(e) =>
          openMenu(e, [
            ...(framebuffer ? sizeItems() : []),
            ...actions.map(({ label, pressed, run }) => ({ label, checked: pressed, run })),
          ])}
      >
        <Ellipsis size={14} />
      </IconButton>
    </div>
  {/if}
  <span class="ml-1.5">
    {#if ended}
      <IconButton size="sm" label="Reconnect" onclick={() => reconnect.start(tab)}>
        <RotateCw size={14} />
      </IconButton>
    {:else}
      <IconButton size="sm" label="Disconnect" onclick={() => tab.close()}>
        <Unplug size={14} />
      </IconButton>
    {/if}
  </span>
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}

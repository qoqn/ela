<script lang="ts">
  import { ENV_COLOR, EnvBadge, LabelTape } from '../../ui';
  import type { SessionTab } from '../../sessions.svelte';
  import SessionActions from './SessionActions.svelte';

  let { tab, focused }: { tab: SessionTab; focused: boolean } = $props();
  const prod = $derived(tab.env === 'prod');
</script>

<div
  class="@container relative flex h-6.5 shrink-0 items-center gap-2.5 pr-1 pl-2"
  style:--rail-color={ENV_COLOR[tab.env]}
  style:background-color="color-mix(in srgb, {ENV_COLOR[tab.env]}
  {focused ? 18 : 9}%, var(--color-panel))"
>
  <LabelTape name={tab.name} user={tab.user} address={tab.address} />
  <span class="min-w-0 truncate text-sm font-medium"><EnvBadge env={tab.env} /></span>
  <SessionActions {tab} />

  <span
    aria-hidden="true"
    class="absolute inset-x-0 bottom-0 {prod ? 'h-0.75' : 'h-0.5'}"
    class:hatched={prod}
    style:background-color={prod ? undefined : ENV_COLOR[tab.env]}
    style:opacity={focused ? 1 : 0.6}
  ></span>
</div>

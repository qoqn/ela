<script lang="ts">
  let {
    name,
    user = null,
    address = '',
  }: { name: string; user?: string | null; address?: string } = $props();
</script>

<span
  title={address || name}
  class="inline-flex h-4.25 max-w-[28ch] min-w-[12ch] items-center gap-1 rounded-sm border-b border-[#c9c1a6] bg-label px-1.5 font-condensed text-sm leading-none font-semibold text-label-ink"
>
  <span class="truncate">{name}</span>
  {#if user}<span class="shrink-0 font-medium opacity-60">{user}</span>{/if}
</span>

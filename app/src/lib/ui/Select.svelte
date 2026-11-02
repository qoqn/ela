<script lang="ts">
  import type { HTMLSelectAttributes } from 'svelte/elements';

  type Props = Omit<HTMLSelectAttributes, 'value'> & {
    value?: string;
    options: { value: string; label: string }[];
  };
  let { value = $bindable(''), options, class: cls = '', ...rest }: Props = $props();
</script>

<select
  {...rest}
  bind:value
  class="h-7 rounded-sm border border-edge bg-chassis px-1.5 outline-none focus:border-signal {cls}"
>
  {#each options as o (o.value)}
    <option value={o.value}>{o.label}</option>
  {/each}
</select>

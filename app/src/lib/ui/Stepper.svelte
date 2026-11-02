<script lang="ts">
  import { Minus, Plus } from '@lucide/svelte';
  import IconButton from './IconButton.svelte';

  type Props = {
    label: string;
    value: number;
    min: number;
    max: number;
    onchange: (value: number) => void;
  };
  let { label, value, min, max, onchange }: Props = $props();
</script>

<div role="group" aria-label={label} class="flex items-center gap-1">
  <IconButton label="Smaller" disabled={value <= min} onclick={() => onchange(value - 1)}>
    <Minus size={14} />
  </IconButton>
  <output class="w-7 text-center tabular">{value}</output>
  <IconButton label="Larger" disabled={value >= max} onclick={() => onchange(value + 1)}>
    <Plus size={14} />
  </IconButton>
</div>

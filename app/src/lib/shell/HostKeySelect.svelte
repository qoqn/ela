<script lang="ts">
  import type { HTMLSelectAttributes } from 'svelte/elements';
  import { Select } from '../ui';
  import { shell } from './shell.svelte';
  import { HOST_KEYS, isHostKeyCode } from './shortcuts';

  let props: Omit<HTMLSelectAttributes, 'value' | 'onchange'> = $props();
</script>

<Select
  {...props}
  options={HOST_KEYS.map((key) => ({ value: key.code, label: key.label }))}
  value={shell.hostKeyCode}
  onchange={(e) =>
    isHostKeyCode(e.currentTarget.value) && void shell.setHostKey(e.currentTarget.value)}
/>

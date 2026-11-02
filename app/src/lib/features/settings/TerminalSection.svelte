<script lang="ts">
  import { Button, Select, SettingRow, Stepper } from '../../ui';
  import { terminalFont } from './terminalFont.svelte';
  import { DEFAULT_SIZE, fontChoices, MAX_SIZE, MIN_SIZE } from './terminalFont';

  terminalFont.loadSystemFonts();

  const choices = $derived(
    fontChoices(terminalFont.system, terminalFont.family, terminalFont.systemLoaded),
  );
</script>

<SettingRow
  label="Font"
  description="The monospaced fonts on this machine."
  error={terminalFont.error &&
    `It works until Ela closes, but it couldn't be saved: ${terminalFont.error}`}
>
  <Select
    class="w-56"
    aria-label="Font"
    options={choices}
    value={terminalFont.family ?? ''}
    onchange={(e) => terminalFont.setFamily(e.currentTarget.value || null)}
  />
</SettingRow>
<SettingRow label="Font size" description="Also from the keyboard, as listed under Shortcuts.">
  {#if terminalFont.size !== DEFAULT_SIZE}
    <Button size="sm" variant="ghost" onclick={() => terminalFont.setSize(DEFAULT_SIZE)}>
      Reset
    </Button>
  {/if}
  <Stepper
    label="Font size"
    value={terminalFont.size}
    min={MIN_SIZE}
    max={MAX_SIZE}
    onchange={(size) => terminalFont.setSize(size)}
  />
</SettingRow>
<div
  class="overflow-hidden rounded-sm border border-line bg-screen px-3 py-2 leading-[1.15] whitespace-pre selectable"
  aria-label="Preview"
  style:font-family={terminalFont.stack}
  style:font-size="{terminalFont.size}px"
>
  <span class="text-[#0dbc79]">root@db-01</span>:<span class="text-[#3b8eea]">~</span
  >{'$ ls -la /etc | grep 0O1lI'}
</div>

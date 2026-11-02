<script lang="ts">
  import { onMount } from 'svelte';
  import { shell } from '../../shell/shell.svelte';
  import { Button, Select, SettingRow } from '../../ui';
  import { idleLabel } from '../vault/idle';
  import { vault } from '../vault/vault.svelte';

  let error = $state('');

  onMount(() => void vault.refresh());

  async function chooseIdle(minutes: number) {
    error = '';
    try {
      await vault.setIdleMinutes(minutes);
    } catch (e) {
      error = String(e);
    }
  }

  function openPanel() {
    shell.section = 'vault';
    shell.sidebarOpen = true;
    shell.settingsOpen = false;
  }
</script>

{#if vault.info && vault.info.state !== 'absent'}
  <SettingRow
    label="Lock after"
    description="How long the Vault stays open without use. Open sessions stay open when it locks."
    {error}
  >
    <Select
      aria-label="Lock after"
      class="w-40"
      value={String(vault.info.idle_minutes)}
      onchange={(e) => void chooseIdle(Number(e.currentTarget.value))}
      options={vault.info.idle_choices.map((minutes) => ({
        value: String(minutes),
        label: idleLabel(minutes),
      }))}
    />
  </SettingRow>
{/if}
<SettingRow
  label="Master password and saved keys"
  description={vault.info?.state === 'absent'
    ? 'No Vault yet. Create one to save passwords with your connections.'
    : 'Lock, change the master password and forget key passphrases in the Vault panel.'}
>
  <Button size="sm" onclick={openPanel}>Open Vault</Button>
</SettingRow>

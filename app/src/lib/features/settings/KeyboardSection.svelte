<script lang="ts">
  import HostKeySelect from '../../shell/HostKeySelect.svelte';
  import { shell } from '../../shell/shell.svelte';
  import { Button, SettingRow } from '../../ui';
</script>

<SettingRow
  label="Host key"
  description="In a remote desktop, tap it alone to release the keyboard, or hold it with a shortcut's key. It never reaches the remote side."
  error={shell.hostKeyProblem}
>
  <HostKeySelect aria-label="Host key" class="w-40" />
</SettingRow>
<SettingRow
  label="Shortcuts"
  description="Every shortcut, in a terminal, a remote desktop and elsewhere."
>
  <Button size="sm" onclick={() => (shell.sheetOpen = true)}>Show shortcuts</Button>
</SettingRow>

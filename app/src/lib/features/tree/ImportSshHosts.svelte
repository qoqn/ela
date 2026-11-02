<script lang="ts">
  import { Button, Dialog } from '../../ui';
  import { saved } from './saved.svelte';

  const close = () => (saved.askingImport = false);
</script>

<Dialog title="Import SSH hosts" onclose={close}>
  <p class="text-sm text-dim">
    Ela reads <code class="font-mono text-text">~/.ssh/config</code> and the files it includes. Each Host
    without a wildcard becomes a connection named after it.
  </p>
  <p class="mt-2 text-sm text-dim">
    Names already saved are left as they are, the file isn't changed, and Match exec commands are
    never run.
  </p>
  {#snippet footer()}
    <Button type="button" onclick={close}>Cancel</Button>
    <Button
      data-autofocus
      variant="primary"
      type="button"
      onclick={() => {
        close();
        void saved.importSshConfig();
      }}>Import</Button
    >
  {/snippet}
</Dialog>

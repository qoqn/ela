<script lang="ts">
  import { Button, TextField } from '../../ui';

  type Props = {
    saved: boolean;
    password: string;
    removing: boolean;
    inheritedFrom?: string;
    emptyHint: string;
  };
  let {
    saved,
    password = $bindable(),
    removing = $bindable(),
    inheritedFrom,
    emptyHint,
  }: Props = $props();

  let replacing = $state(false);
  const hasSaved = $derived(saved && !removing);
</script>

<div class="text-sm text-dim">
  {#if hasSaved && !replacing}
    <p>Password</p>
    <div class="mt-1 flex h-7 items-center gap-2">
      <span class="flex-1 text-base text-text">Saved in the Vault</span>
      <Button size="sm" type="button" onclick={() => (replacing = true)}>Replace</Button>
      <Button size="sm" type="button" onclick={() => (removing = true)}>Remove</Button>
    </div>
  {:else}
    <label class="block">
      Password
      <TextField
        class="mt-1 text-base text-text"
        type="password"
        bind:value={password}
        placeholder={replacing ? 'New password' : ''}
      />
    </label>
    <p class="mt-1">
      {#if replacing}
        The saved password stays until you save this form.
      {:else if inheritedFrom}
        Empty uses the password saved on {inheritedFrom}.
      {:else}
        {emptyHint}
      {/if}
    </p>
  {/if}
</div>

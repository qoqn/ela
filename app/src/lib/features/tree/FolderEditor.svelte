<script lang="ts">
  import { tick } from 'svelte';
  import type { SavedFolder } from '../../ipc';
  import { workspace } from '../../sessions.svelte';
  import { Button, Dialog, EnvPicker, Tabs, TextField, type Env } from '../../ui';
  import OptionFields from './OptionFields.svelte';
  import PasswordField from './PasswordField.svelte';
  import { saved } from './saved.svelte';
  import {
    folderChain,
    groupOptions,
    inheritedSettings,
    listKeysOf,
    optionsAsText,
    optionsToSave,
    parsePort,
  } from './tree';

  let { draft, isNew }: { draft: SavedFolder; isNew: boolean } = $props();

  // The form starts from the draft and is only written back on save.
  // svelte-ignore state_referenced_locally
  const initial = draft;
  let name = $state(initial.name);
  let port = $state(initial.port?.toString() ?? '');
  let username = $state(initial.username ?? '');
  let environment = $state<Env | null>(initial.environment ?? null);
  // Every Protocol's lists, since the editor may switch Protocol.
  let optionText = $state(
    optionsAsText(initial.options, listKeysOf(workspace.protocols.map((p) => p.settings_schema))),
  );
  // The password goes to Rust once and isn't kept here (ADR-0002).
  let password = $state('');
  let removing = $state(false);
  let error = $state('');
  let portError = $state('');
  let nameError = $state('');
  let busy = $state(false);

  const options = $derived(
    groupOptions(
      workspace.protocols
        .filter((p) => p.listed)
        .map((p) => ({ id: p.id, name: p.display_name, schema: p.settings_schema })),
    ),
  );
  let tab = $state('general');
  const tabs = $derived([
    { id: 'general', label: 'General', invalid: !!(portError || nameError) },
    { id: 'sign_in', label: 'Sign-in' },
    ...options.own.map((own) => ({ id: own.id, label: `${own.name} options` })),
    ...(options.advanced.length ? [{ id: 'advanced', label: 'Advanced' }] : []),
  ]);
  const inherited = $derived(inheritedSettings(initial.parent, saved.folders));
  const place = $derived(
    folderChain(initial.parent, saved.folders)
      .map((folder) => folder.name)
      .reverse()
      .join(' / '),
  );

  async function save(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    const parsed = parsePort(port);
    portError = 'error' in parsed ? parsed.error : '';
    nameError = name.trim() ? '' : 'A folder needs a name.';
    if ('error' in parsed || nameError) {
      tab = 'general';
      await tick();
      document.querySelector<HTMLElement>('#folder-form [aria-invalid=true]')?.focus();
      return;
    }
    busy = true;
    try {
      const fields = [
        ...options.route,
        ...options.signIn,
        ...options.advanced,
        ...options.own.flatMap((own) => own.fields),
      ];
      const known = new Set(fields.map(([key]) => key));
      const done = await saved.saveWithCredential(
        {
          label: name.trim() || 'this folder',
          current: initial.credential,
          password,
          removing,
        },
        (credential) =>
          saved.saveFolder({
            ...initial,
            name: name.trim(),
            port: parsed.port,
            username: username.trim() || undefined,
            environment: environment ?? undefined,
            credential,
            options: optionsToSave(initial.options, known, fields, optionText) as never,
          }),
      );
      if (!done) return;
      password = '';
      saved.editing = null;
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  title={isNew ? 'New folder' : `Edit ${initial.name}`}
  onclose={() => (saved.editing = null)}
  width="56ch"
>
  <form id="folder-form" onsubmit={save}>
    <p class="mb-2 text-sm text-dim">
      {place ? `In ${place}. ` : ''}Everything in this folder takes these values unless it sets its
      own.
    </p>
    <Tabs {tabs} bind:selected={tab} label="Folder settings">
      {#snippet panel(id)}
        <div class="space-y-3">
          {#if id === 'general'}
            <label class="block text-sm text-dim">
              Name
              <TextField
                class="mt-1 text-base text-text"
                data-autofocus
                bind:value={name}
                error={nameError}
              />
            </label>
            <label class="block w-28 text-sm text-dim">
              Port
              <TextField
                class="mt-1 text-base text-text"
                mono
                bind:value={port}
                placeholder={inherited.port?.toString() ?? ''}
                error={portError}
              />
            </label>
            <OptionFields
              fields={options.route}
              bind:text={optionText}
              inherited={inherited.options}
            />
            <div class="text-sm text-dim">
              <span id="environment-label">Environment</span>
              <div class="mt-1 text-base">
                <EnvPicker bind:value={environment} inherited={inherited.environment ?? 'none'} />
              </div>
            </div>
          {:else if id === 'sign_in'}
            <label class="block text-sm text-dim">
              Username
              <TextField
                class="mt-1 text-base text-text"
                bind:value={username}
                placeholder={inherited.username != null
                  ? `${inherited.username}, from ${inherited.folderName('username')}`
                  : ''}
              />
            </label>
            <PasswordField
              saved={!!initial.credential}
              bind:password
              bind:removing
              inheritedFrom={inherited.credential ? inherited.folderName('credential') : undefined}
              emptyHint="Saved in the Vault. Leave it empty if this folder has no password of its own."
            />
            <OptionFields
              fields={options.signIn}
              bind:text={optionText}
              inherited={inherited.options}
            />
          {:else if id === 'advanced'}
            <OptionFields
              fields={options.advanced}
              bind:text={optionText}
              inherited={inherited.options}
            />
          {:else}
            {#each options.own.filter((own) => own.id === id) as own (own.id)}
              <OptionFields
                fields={own.fields}
                bind:text={optionText}
                inherited={inherited.options}
              />
            {/each}
          {/if}
        </div>
      {/snippet}
    </Tabs>
  </form>
  {#snippet footer()}
    {#if error}<p class="mr-auto self-center text-sm text-danger-text" role="alert">{error}</p>{/if}
    <Button type="button" onclick={() => (saved.editing = null)}>Cancel</Button>
    <Button variant="primary" type="submit" form="folder-form" disabled={busy}>Save</Button>
  {/snippet}
</Dialog>

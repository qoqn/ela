<script lang="ts">
  import { tick } from 'svelte';
  import { commands } from '../../bindings';
  import type { Outside, SavedConnection } from '../../ipc';
  import { workspace } from '../../sessions.svelte';
  import { Button, Dialog, EnvPicker, Select, Tabs, TextField, type Env } from '../../ui';
  import OptionFields from './OptionFields.svelte';
  import TagField from './TagField.svelte';
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

  let { draft, isNew }: { draft: SavedConnection; isNew: boolean } = $props();

  // The form starts from the draft and is only written back on save.
  // svelte-ignore state_referenced_locally
  const initial = draft;
  let name = $state(initial.name);
  let protocol = $state(initial.protocol);
  let host = $state(initial.host);
  let port = $state(initial.port?.toString() ?? '');
  let username = $state(initial.username ?? '');
  let environment = $state<Env | null>(initial.environment ?? null);
  let tags = $state([...(initial.tags ?? [])]);
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
  let hostError = $state('');
  let busy = $state(false);

  const protocols = $derived(
    workspace.protocols.filter((p) => p.listed || p.id === initial.protocol),
  );
  const info = $derived(workspace.protocol(protocol));
  const options = $derived(
    groupOptions(
      info ? [{ id: info.id, name: info.display_name, schema: info.settings_schema }] : [],
    ),
  );
  const schema = $derived([
    ...options.route,
    ...options.signIn,
    ...options.advanced,
    ...options.own.flatMap((own) => own.fields),
  ]);
  let tab = $state('general');
  const tabs = $derived([
    { id: 'general', label: 'General', invalid: !!(portError || nameError || hostError) },
    { id: 'sign_in', label: 'Sign-in' },
    ...options.own.map((own) => ({ id: 'options', label: `${own.name} options` })),
    ...(options.advanced.length ? [{ id: 'advanced', label: 'Advanced' }] : []),
  ]);
  // Switching to a Protocol without options of its own leaves no tab to stay on.
  $effect(() => {
    if (!tabs.some((t) => t.id === tab)) tab = 'general';
  });
  let outside = $state<Outside | null>(null);
  $effect(() => {
    const [forProtocol, forHost] = [protocol, host.trim()];
    if (!forHost) return void (outside = null);
    const timer = setTimeout(() => {
      commands
        .outsideSettings(forProtocol, forHost)
        .then((found) => (outside = found))
        .catch(() => (outside = null));
    }, 250);
    return () => clearTimeout(timer);
  });
  const inherited = $derived(inheritedSettings(initial.folder, saved.folders, outside));
  const place = $derived(
    folderChain(initial.folder, saved.folders)
      .map((folder) => folder.name)
      .reverse()
      .join(' / '),
  );

  function hint(value: unknown, from: string | undefined, fallback: string) {
    return value != null && from ? `${value}, from ${from}` : fallback;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    const parsed = parsePort(port);
    portError = 'error' in parsed ? parsed.error : '';
    nameError = name.trim() ? '' : 'A connection needs a name.';
    hostError = host.trim() ? '' : 'A connection needs a host.';
    if ('error' in parsed || nameError || hostError) {
      tab = 'general';
      await tick();
      document.querySelector<HTMLElement>('#connection-form [aria-invalid=true]')?.focus();
      return;
    }
    busy = true;
    try {
      const known = new Set(
        workspace.protocols.flatMap((p) => Object.keys((p.settings_schema ?? {}) as object)),
      );
      const done = await saved.saveWithCredential(
        {
          label: name.trim() || 'this connection',
          current: initial.credential,
          password,
          removing,
        },
        (credential) =>
          saved.saveConnection({
            ...initial,
            name: name.trim(),
            protocol,
            host: host.trim(),
            port: parsed.port,
            username: username.trim() || undefined,
            environment: environment ?? undefined,
            credential,
            tags: tags.length ? [...tags] : undefined,
            options: optionsToSave(initial.options, known, schema, optionText) as never,
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
  title={isNew ? 'New connection' : `Edit ${initial.name}`}
  onclose={() => (saved.editing = null)}
  width="56ch"
>
  <form id="connection-form" onsubmit={save}>
    {#if place}<p class="mb-2 text-sm text-dim">In {place}</p>{/if}
    <Tabs {tabs} bind:selected={tab} label="Connection settings">
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
            <div class="grid grid-cols-[7rem_1fr_6rem] gap-2">
              <label class="block text-sm text-dim">
                Protocol
                <Select
                  class="mt-1 w-full text-base text-text"
                  bind:value={protocol}
                  options={protocols.map((p) => ({ value: p.id, label: p.display_name }))}
                />
              </label>
              <label class="block text-sm text-dim">
                Host
                <TextField
                  class="mt-1 text-base text-text"
                  mono
                  bind:value={host}
                  error={hostError}
                  placeholder="db-01.internal"
                />
              </label>
              <label class="block text-sm text-dim">
                Port
                <TextField
                  class="mt-1 text-base text-text"
                  mono
                  bind:value={port}
                  placeholder={(inherited.port ?? info?.default_port)?.toString() ?? ''}
                  error={portError}
                />
              </label>
              {#if outside?.hostname}
                <p class="col-span-full text-sm text-dim">
                  Connects to <span class="font-mono break-all"
                    >{outside.hostname}{outside.settings.port
                      ? `:${outside.settings.port}`
                      : ''}</span
                  >, from {outside.source}.
                </p>
              {/if}
            </div>
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
            <div class="text-sm text-dim">
              Tags
              <TagField bind:tags />
            </div>
          {:else if id === 'sign_in'}
            <label class="block text-sm text-dim">
              Username
              <TextField
                class="mt-1 text-base text-text"
                bind:value={username}
                placeholder={hint(
                  inherited.username,
                  inherited.folderName('username'),
                  'Your local username',
                )}
              />
            </label>
            <PasswordField
              saved={!!initial.credential}
              bind:password
              bind:removing
              inheritedFrom={inherited.credential ? inherited.folderName('credential') : undefined}
              emptyHint="Saved in the Vault. Leave it empty to be asked each time."
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
            {#each options.own as own (own.id)}
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
    <Button variant="primary" type="submit" form="connection-form" disabled={busy}>Save</Button>
  {/snippet}
</Dialog>

<script lang="ts">
  import type { OpenRequest } from '../../ipc';
  import { workspace } from '../../sessions.svelte';
  import { shell } from '../../shell/shell.svelte';
  import { Dialog, EnvRail, SectionHeader, type Env } from '../../ui';

  const benchmarks: { name: string; detail: string; request: OpenRequest }[] = [
    {
      name: 'Terminal flood',
      detail: '50 MB of coloured text',
      request: { protocol: 'bench-term', host: 'bench', options: { mb: 50 } },
    },
    {
      name: 'Terminal flood, large',
      detail: '200 MB of coloured text',
      request: { protocol: 'bench-term', host: 'bench', options: { mb: 200 } },
    },
    {
      name: 'Full HD, every pixel',
      detail: '1920×1080 at 60 fps, like video',
      request: {
        protocol: 'bench-fb',
        host: 'bench',
        options: { mode: 'full', width: 1920, height: 1080, fps: 60 },
      },
    },
    {
      name: 'Full HD, moving box',
      detail: '1920×1080 at 60 fps, like a desktop',
      request: {
        protocol: 'bench-fb',
        host: 'bench',
        options: { mode: 'partial', width: 1920, height: 1080, fps: 60 },
      },
    },
    {
      name: '4K, every pixel',
      detail: '3840×2160 at 30 fps',
      request: {
        protocol: 'bench-fb',
        host: 'bench',
        options: { mode: 'full', width: 3840, height: 2160, fps: 30 },
      },
    },
    {
      name: 'Full HD, every pixel, lossless',
      detail: 'The same without JPEG',
      request: {
        protocol: 'bench-fb',
        host: 'bench',
        options: { mode: 'full', width: 1920, height: 1080, fps: 60, image_quality: 'lossless' },
      },
    },
    {
      name: '4K, every pixel, lossless',
      detail: 'The same without JPEG',
      request: {
        protocol: 'bench-fb',
        host: 'bench',
        options: { mode: 'full', width: 3840, height: 2160, fps: 30, image_quality: 'lossless' },
      },
    },
  ];

  const fixtures: { name: string; request: OpenRequest; env: Env }[] = [
    {
      name: 'SSH test server',
      request: { protocol: 'ssh', host: 'localhost', port: 2222, username: 'ela' },
      env: 'dev',
    },
    {
      name: 'RDP test desktop',
      request: { protocol: 'rdp', host: 'localhost', port: 3389, username: 'ela' },
      env: 'dev',
    },
  ];
  function open(request: OpenRequest, meta: { name: string; env: Env; record?: boolean }) {
    shell.devToolsOpen = false;
    workspace.open(request, meta);
  }
</script>

<Dialog title="Development tools" onclose={() => (shell.devToolsOpen = false)} width="60ch">
  <div class="-mx-4 -my-3 py-2">
    <SectionHeader title="Test servers" />
    <ul>
      {#each fixtures as f (f.name)}
        <li>
          {@render item(
            f.name,
            `${f.request.protocol}://${f.request.username}@${f.request.host}:${f.request.port}`,
            f.env,
            () => open(f.request, { name: f.name, env: f.env }),
          )}
        </li>
      {/each}
    </ul>
    <p class="mx-3 mt-1 mb-3 text-sm text-dim">
      Start them with <code class="font-mono text-xs text-text selectable">docker compose up</code>
      in the <code class="font-mono text-xs text-text">dev</code> folder. Clicking one connects as ela.
    </p>

    <SectionHeader title="Benchmarks" />
    <ul>
      {#each benchmarks as b (b.name)}
        <li>
          {@render item(b.name, b.detail, 'lab', () =>
            open(b.request, { name: b.name, env: 'lab', record: true }),
          )}
        </li>
      {/each}
    </ul>
  </div>
</Dialog>

{#snippet item(name: string, detail: string, env: Env, run: () => void)}
  <button
    data-row
    class="flex h-10.5 w-full items-stretch py-1 text-left hover:bg-raised"
    onclick={run}
  >
    <span class="flex min-w-0 flex-1 flex-col justify-center pl-3">
      <span class="truncate">{name}</span>
      <span class="truncate font-mono text-xs text-dim">{detail}</span>
    </span>
    <EnvRail {env} orientation="vertical" />
  </button>
{/snippet}

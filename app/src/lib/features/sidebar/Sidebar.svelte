<script lang="ts">
  import { FolderPlus, Plus } from '@lucide/svelte';
  import { IconButton, Panel, Separator } from '../../ui';
  import ConnectionTree from '../tree/ConnectionTree.svelte';
  import RecentList from '../tree/RecentList.svelte';
  import TrashRow from '../tree/TrashRow.svelte';
  import { saved } from '../tree/saved.svelte';
  import SearchConnect from './SearchConnect.svelte';

  const folderName = $derived.by(() => {
    const id = saved.currentFolder();
    return id ? saved.folders.find((folder) => folder.id === id)?.name : undefined;
  });
  const into = $derived(folderName ? ` in ${folderName}` : '');
</script>

<Panel title="Connections">
  {#snippet actions()}
    <IconButton label="New connection{into}" onclick={() => saved.newConnection()}>
      <Plus size={15} />
    </IconButton>
    <IconButton label="New folder{into}" onclick={() => saved.newFolder()}>
      <FolderPlus size={15} />
    </IconButton>
  {/snippet}

  <SearchConnect />

  <Separator />
  <ConnectionTree />
  <RecentList />
  <TrashRow />
</Panel>

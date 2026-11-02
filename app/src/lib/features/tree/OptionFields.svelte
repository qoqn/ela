<script lang="ts">
  import { Select, TextArea, TextField } from '../../ui';
  import { listLines, type OptionEntry } from './tree';

  type Props = {
    fields: OptionEntry[];
    text: Record<string, string>;
    inherited: Record<string, unknown>;
  };
  let { fields, text = $bindable(), inherited }: Props = $props();

  const shown = (labels: Record<string, string> | undefined, value: unknown) =>
    labels?.[String(value)] ?? String(value);
</script>

{#each fields as [key, field] (key)}
  {@const fallback = inherited[key] ?? field.default}
  <label class="block text-sm text-dim">
    {field.label ?? key}
    {#if field.type === 'enum'}
      <Select
        class="mt-1 w-full text-base text-text"
        value={text[key] ?? ''}
        onchange={(e) => (text[key] = e.currentTarget.value)}
        options={[
          {
            value: '',
            label: fallback != null ? `Default (${shown(field.labels, fallback)})` : 'Default',
          },
          ...(field.values ?? []).map((value) => ({ value, label: shown(field.labels, value) })),
        ]}
      />
    {:else if field.type === 'list'}
      <TextArea
        class="mt-1 text-base text-text"
        rows={2}
        mono={field.mono}
        value={text[key] ?? ''}
        oninput={(e) => (text[key] = e.currentTarget.value)}
        placeholder={fallback != null ? listLines(fallback) : ''}
      />
    {:else}
      <TextField
        class="mt-1 text-base text-text"
        mono={field.mono}
        value={text[key] ?? ''}
        oninput={(e) => (text[key] = e.currentTarget.value)}
        placeholder={fallback != null ? String(fallback) : ''}
      />
    {/if}
    {#if field.hint}<span class="mt-1 block">{field.hint}</span>{/if}
  </label>
{/each}

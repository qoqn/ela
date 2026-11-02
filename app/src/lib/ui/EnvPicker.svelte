<script lang="ts">
  import { ENV_COLOR, ENV_NAME, type Env } from './types';

  type Props = {
    value?: Env | null;
    envs?: Env[];
    inherited?: Env;
  };
  let {
    value = $bindable('none'),
    envs = ['none', 'dev', 'staging', 'prod', 'lab'],
    inherited,
  }: Props = $props();

  const choices = $derived<(Env | null)[]>(inherited ? [null, ...envs] : envs);
  const shown = $derived(value ?? inherited ?? 'none');

  function onKeydown(e: KeyboardEvent) {
    const step =
      e.key === 'ArrowRight' || e.key === 'ArrowDown'
        ? 1
        : e.key === 'ArrowLeft' || e.key === 'ArrowUp'
          ? -1
          : 0;
    if (!step) return;
    e.preventDefault();
    value = choices[(choices.indexOf(value) + step + choices.length) % choices.length];
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }
</script>

<div class="flex w-full items-center gap-2.5">
  <div
    role="radiogroup"
    aria-label="Environment"
    class="flex gap-1"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    {#if inherited}
      <button
        type="button"
        role="radio"
        aria-checked={value === null}
        aria-label="From folder: {ENV_NAME[inherited]}"
        title="From folder: {ENV_NAME[inherited]}"
        tabindex={value === null ? 0 : -1}
        class="grid size-5.5 place-items-center rounded-sm border transition-colors duration-(--motion-fast) focus-visible:outline-offset-0 {value ===
        null
          ? 'border-text'
          : 'border-transparent hover:border-line'}"
        onclick={() => (value = null)}
      >
        <span
          class="size-3 rounded-xs border border-dashed"
          style:border-color={ENV_COLOR[inherited]}
        ></span>
      </button>
    {/if}
    {#each envs as env (env)}
      {@const selected = env === value}
      <button
        type="button"
        role="radio"
        aria-checked={selected}
        aria-label={ENV_NAME[env]}
        title={ENV_NAME[env]}
        tabindex={selected ? 0 : -1}
        class="grid size-5.5 place-items-center rounded-sm border transition-colors duration-(--motion-fast) focus-visible:outline-offset-0 {selected
          ? 'border-text'
          : 'border-transparent hover:border-line'}"
        onclick={() => (value = env)}
      >
        <span
          class="size-3 rounded-xs"
          class:hatched={env === 'prod'}
          style:--rail-color={ENV_COLOR[env]}
          style:background-color={env === 'prod' ? undefined : ENV_COLOR[env]}
        ></span>
      </button>
    {/each}
  </div>
  <span class="min-w-0 flex-1 truncate text-sm text-dim"
    >{value === null ? `From folder: ${ENV_NAME[shown]}` : ENV_NAME[shown]}</span
  >
</div>

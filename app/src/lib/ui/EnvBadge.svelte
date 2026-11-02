<script lang="ts">
  import { ENV_COLOR, ENV_NAME, type Env } from './types';

  let { env, production }: { env: Env; production?: boolean } = $props();
  const isProd = $derived(production ?? env === 'prod');
</script>

<span class="inline-flex items-center gap-1.5 whitespace-nowrap">
  <span
    aria-hidden="true"
    class="size-2.5 rounded-xs"
    class:hatched={isProd}
    style:--rail-color={ENV_COLOR[env]}
    style:background-color={isProd ? undefined : ENV_COLOR[env]}
  ></span>
  <span>{ENV_NAME[env]}</span>
</span>

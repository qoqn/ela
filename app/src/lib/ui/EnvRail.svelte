<script lang="ts">
  import { ENV_COLOR, ENV_NAME, type Env } from './types';

  type Props = {
    env: Env;
    production?: boolean;
    orientation: 'vertical' | 'top';
    dimmed?: boolean;
  };
  let { env, production, orientation, dimmed = false }: Props = $props();
  const isProd = $derived(production ?? env === 'prod');
</script>

<span
  aria-hidden="true"
  title={ENV_NAME[env]}
  class="block shrink-0 {orientation === 'vertical' ? 'h-full w-0.75' : 'h-0.75 w-full'}"
  class:hatched={isProd}
  style:--rail-color={ENV_COLOR[env]}
  style:background-color={isProd ? undefined : ENV_COLOR[env]}
  style:opacity={dimmed ? 0.45 : 1}
></span>

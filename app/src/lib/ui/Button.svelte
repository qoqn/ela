<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Props = HTMLButtonAttributes & {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md';
    children: Snippet;
  };
  let { variant = 'secondary', size = 'md', children, class: cls = '', ...rest }: Props = $props();

  const variants = {
    primary: 'bg-signal-strong text-on-accent font-medium hover:brightness-110',
    secondary: 'bg-raised text-text hover:bg-line',
    ghost: 'text-dim hover:text-text hover:bg-raised',
    danger: 'bg-danger text-on-accent font-medium hover:brightness-110',
  };
  const sizes = { sm: 'h-6 px-2 text-sm', md: 'h-7 px-3' };
</script>

<button
  {...rest}
  class="inline-flex items-center justify-center gap-1.5 rounded-sm whitespace-nowrap transition-[background-color,filter,color] duration-(--motion-fast) disabled:pointer-events-none disabled:opacity-40 {variants[
    variant
  ]} {sizes[size]} {cls}"
>
  {@render children()}
</button>

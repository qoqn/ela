const collator = new Intl.Collator(undefined, { numeric: true });

export const compareNames = (a: string, b: string) => collator.compare(a, b);

import type { FindResults } from './find';

export class TerminalFind {
  open = $state(false);
  query = $state('');
  matchCase = $state(false);
  regex = $state(false);
  results = $state<FindResults>({ index: -1, count: 0 });
  show: () => void = () => {};
}

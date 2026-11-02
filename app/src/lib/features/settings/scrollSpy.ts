// Sections above the line have scrolled past the top. The last of them is the one being read.
export function currentSection(tops: number[], line: number): number {
  let current = 0;
  tops.forEach((top, index) => {
    if (top <= line) current = index;
  });
  return current;
}

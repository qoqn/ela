import { describe, expect, test } from 'bun:test';
import {
  addTags,
  allTags,
  cleanTag,
  filtersByTag,
  hasTag,
  parseFilter,
  searchIntent,
  toggleTagFilter,
} from './tags';

describe('parseFilter', () => {
  test('splits Tags from words and ignores a lone #', () => {
    expect(parseFilter('  DB #EU-west # ')).toEqual({ tags: ['eu-west'], words: ['db'] });
  });
});

describe('hasTag', () => {
  test('matches the start of a Tag, so typing narrows as it goes', () => {
    expect(hasTag(['eu-west'], 'eu')).toBe(true);
    expect(hasTag(['eu-west'], 'west')).toBe(false);
    expect(hasTag(undefined, 'eu')).toBe(false);
  });

  test('finds a Tag saved with a space through its dashed form', () => {
    expect(hasTag(['eu west'], 'eu-west')).toBe(true);
  });
});

describe('addTags', () => {
  test('cleans, splits on commas and keeps one of each', () => {
    expect(addTags(['db'], ' #eu west, DB ,,new')).toEqual(['db', 'eu-west', 'new']);
  });
});

describe('allTags', () => {
  test('lists each Tag once, in name order with numbers as numbers', () => {
    const connections = [{ tags: ['rack-10', 'db'] }, { tags: ['DB', 'rack-2'] }, {}];
    expect(allTags(connections)).toEqual(['db', 'rack-2', 'rack-10']);
  });
});

describe('toggleTagFilter', () => {
  test('adds the term after what is typed, and takes it out again', () => {
    expect(toggleTagFilter('db', 'eu-west')).toBe('db #eu-west');
    expect(toggleTagFilter('db #EU-west', 'eu-west')).toBe('db');
    expect(filtersByTag('db #eu-west', 'EU-West')).toBe(true);
  });
});

describe('cleanTag', () => {
  test('drops the # and turns spaces into dashes', () => {
    expect(cleanTag(' ##eu  west ')).toBe('eu-west');
  });
});

describe('searchIntent', () => {
  test('nothing typed', () => {
    expect(searchIntent('  ', 3)).toBe('idle');
  });

  test('a saved name wins over reading it as a host', () => {
    expect(searchIntent('db-01.internal', 1)).toBe('search');
  });

  test('@ and : always mean an address, whatever matches', () => {
    expect(searchIntent('root@db-01', 2)).toBe('address');
    expect(searchIntent('rdp://win-ad', 2)).toBe('address');
    expect(searchIntent('10.0.0.5:2222', 2)).toBe('address');
  });

  test('anything else with no match can only be an address', () => {
    expect(searchIntent('10.0.0.5', 0)).toBe('address');
  });

  test('a Tag or several words are never an address', () => {
    expect(searchIntent('#nothing', 0)).toBe('search');
    expect(searchIntent('no such', 0)).toBe('search');
  });
});

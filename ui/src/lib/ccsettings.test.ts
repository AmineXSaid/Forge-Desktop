import { describe, expect, it } from 'vitest';
import { effective, getPath, mergedEnv, mergedList, overriddenBy, searchTabs, SETTINGS_INDEX, TABS } from './ccsettings';
import type { CcSettings } from './ipc';

const all: CcSettings = {
  user: { model: 'opus', language: 'french', permissions: { allow: ['Read'] }, env: { A: '1', B: 'u' } },
  project: { model: 'sonnet', permissions: { allow: ['Bash(npm test)'], deny: ['Bash(rm:*)'] }, env: { B: 'p' } },
  local: { sandbox: { enabled: true }, env: { C: 'l' } },
  hasProject: true,
  errors: [],
};

describe('scopes', () => {
  it('reads dotted keys and treats null as absent', () => {
    expect(getPath(all.local, 'sandbox.enabled')).toBe(true);
    expect(getPath(all.local, 'sandbox.network.httpProxyPort')).toBeUndefined();
    expect(getPath({ a: null }, 'a')).toBeUndefined();
    expect(getPath({ a: ['x'] }, 'a.0')).toBeUndefined();
  });
  it('local beats workspace beats user', () => {
    expect(effective(all, 'model')).toEqual({ value: 'sonnet', from: 'project' });
    expect(effective(all, 'language')).toEqual({ value: 'french', from: 'user' });
    expect(effective(all, 'sandbox.enabled')).toEqual({ value: true, from: 'local' });
    expect(effective(all, 'outputStyle')).toEqual({ value: undefined, from: null });
  });
  it('says which scope hides a value written lower down', () => {
    expect(overriddenBy(all, 'model', 'user')).toBe('project');
    expect(overriddenBy(all, 'model', 'project')).toBeNull();
    expect(overriddenBy(all, 'model', 'local')).toBeNull();
  });
  it('merges lists and environment variables the way the CLI does', () => {
    expect(mergedList(all, 'permissions.allow')).toEqual([{ item: 'Bash(npm test)', from: 'project' }, { item: 'Read', from: 'user' }]);
    expect(mergedEnv(all)).toEqual([
      { name: 'A', value: '1', from: 'user', shadowed: [] },
      { name: 'B', value: 'p', from: 'project', shadowed: ['user'] },
      { name: 'C', value: 'l', from: 'local', shadowed: [] },
    ]);
  });
});

describe('settings search', () => {
  it('every tab is indexed', () => {
    for (const t of TABS) expect(SETTINGS_INDEX[t.id]?.length, t.id).toBeGreaterThan(0);
  });
  it('ranks the tab name first, then settings', () => {
    expect(searchTabs('', TABS)).toHaveLength(TABS.length);
    expect(searchTabs('hooks', TABS)[0].id).toBe('hooks');
    expect(searchTabs('proxy', TABS).map((t) => t.id)).toEqual(['network', 'sandbox']);
    expect(searchTabs('claude.md', TABS)[0].id).toBe('memory');
    expect(searchTabs('effort', TABS)[0].id).toBe('models');
    expect(searchTabs('zzz', TABS)).toEqual([]);
  });
});

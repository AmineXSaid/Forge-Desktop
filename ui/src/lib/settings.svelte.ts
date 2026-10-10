// The Settings window's state: which tab, which scope, the search, and the
// Claude Code settings files as Rust last read them. Every write goes through
// `cc_settings_set` (Rust checks the key and value) and re-reads the files, so
// what is shown is always what is on disk.

import { call, type CcSettings, type Extensions, type Scope } from './ipc';
import type { TabId } from './ccsettings';
import { toast } from './store.svelte';

const EMPTY: CcSettings = { user: {}, project: {}, local: {}, hasProject: false, trusted: true, errors: [] };

class SettingsWindow {
  open = $state(false);
  tab = $state<TabId>('general');
  scope = $state<Scope>('user');
  query = $state('');
  cc = $state<CcSettings>(EMPTY);
  ext = $state<Extensions | null>(null);
  loaded = $state(false);

  show(tab?: TabId) {
    if (tab) this.tab = tab;
    this.open = true;
    this.refresh();
  }

  close() {
    this.open = false;
    this.query = '';
  }

  async refresh() {
    try {
      const [cc, ext] = await Promise.all([call('cc_settings_get'), call('extensions_list')]);
      this.cc = cc;
      this.ext = ext;
      // Workspace and Local need an open project.
      if (!cc.hasProject && this.scope !== 'user') this.scope = 'user';
      this.loaded = true;
    } catch (e) {
      toast(String(e), 'error');
    }
  }

  /** Write one key in the chosen scope; `null` removes it (reset). */
  async set(key: string, value: unknown, scope: Scope = this.scope): Promise<boolean> {
    try {
      await call('cc_settings_set', { scope, key, value });
      this.cc = await call('cc_settings_get');
      return true;
    } catch (e) {
      toast(String(e), 'error');
      return false;
    }
  }

  reset(key: string, scope: Scope = this.scope) {
    return this.set(key, null, scope);
  }
}

export const settingsWindow = new SettingsWindow();

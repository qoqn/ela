import { emit } from '@tauri-apps/api/event';
import { fileBrowserOf } from '../lib/features/files/files.svelte';
import { knownHosts } from '../lib/features/knownhosts/knownHosts.svelte';
import { reconnect } from '../lib/features/sessions/reconnect.svelte';
import { recent } from '../lib/features/tree/recent.svelte';
import { saved } from '../lib/features/tree/saved.svelte';
import { vault } from '../lib/features/vault/vault.svelte';
import { workspace, type SessionTab } from '../lib/sessions.svelte';
import type { PromptReply } from '../lib/ipc';
import { terminalFont } from '../lib/features/settings/terminalFont.svelte';
import { shell } from '../lib/shell/shell.svelte';

const expandAll = () => saved.folders.forEach((folder) => saved.setExpanded(folder.id, true));
const named = (name: string) => saved.connections.find((c) => c.name === name)!;
const folderNamed = (name: string) => saved.folders.find((f) => f.name === name)!;
const openEditorWithPassword = () => {
  const input = document.querySelector<HTMLInputElement>('#connection-form input[type=password]');
  if (input) {
    input.value = 'hunter22';
    input.dispatchEvent(new Event('input'));
  }
  document.querySelector<HTMLFormElement>('#connection-form')?.requestSubmit();
};

const submitWrongMaster = () => {
  const inputs = document.querySelectorAll<HTMLInputElement>('#change-master-form input');
  ['wrong', 'a new password', 'a new password'].forEach((value, index) => {
    inputs[index].value = value;
    inputs[index].dispatchEvent(new Event('input'));
  });
  document.querySelector<HTMLFormElement>('#change-master-form')?.requestSubmit();
};
const clickTab = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>('[role=tab]')]
    .find((tab) => tab.textContent?.trim() === label)
    ?.click();
const clickButton = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>('[role=dialog] button')]
    .find((button) => button.textContent?.trim() === label)
    ?.click();

function openTerminalMenu(screen: Element, withSelection: boolean) {
  const box = screen.getBoundingClientRect();
  const at = { clientX: box.left + 250, clientY: box.top + 28, bubbles: true };
  if (withSelection) {
    screen.dispatchEvent(new MouseEvent('mousedown', { ...at, detail: 2 }));
    screen.dispatchEvent(new MouseEvent('mouseup', { ...at, detail: 2 }));
  }
  const menuAt = { ...at, clientX: at.clientX + 40, clientY: at.clientY + 12 };
  screen.dispatchEvent(new MouseEvent('contextmenu', { ...menuAt, button: 2 }));
}

async function loadTreeScene(name: string) {
  await saved.load();
  expandAll();
  const db = named('db-01');
  const dbRef = { kind: 'connection', id: db.id } as const;
  if (name !== 'treefilter' && name !== 'treeroot') saved.active = `connection:${db.id}`;
  if (name === 'treeroot') {
    setTimeout(() => {
      const tree = document.querySelector('[role=tree]')!;
      const box = tree.getBoundingClientRect();
      const at = { clientX: box.left + 60, clientY: box.bottom - 120, bubbles: true };
      tree.dispatchEvent(new MouseEvent('contextmenu', at));
    }, 300);
  }
  if (name === 'treerecent') void saved.connect(db.id);
  if (name === 'treerecentfolded') recent.setFolded(true);
  if (name === 'treerecentmenu') {
    setTimeout(() => {
      const row = document.querySelectorAll('[aria-label="Recent connections"] button')[1];
      const box = row!.getBoundingClientRect();
      const at = { clientX: box.left + 90, clientY: box.top + 14, bubbles: true };
      row!.dispatchEvent(new MouseEvent('contextmenu', at));
    }, 400);
  }
  if (name === 'treerecentpalette') setTimeout(() => (shell.palette = 'connect'), 300);
  if (name === 'treefilter') saved.search('db');
  if (name === 'treenomatch') saved.search('prod db');
  if (name === 'treesearchaddress') saved.search('root@10.0.0.5');
  if (name === 'treesearchtag') saved.search('#eu-west');
  if (name === 'treesearchfocus') requestAnimationFrame(() => shell.focusSearch());
  if (name === 'treerename') saved.renaming = `connection:${db.id}`;
  if (name === 'treeedit') saved.edit(dbRef);
  if (name === 'treeeditrdp') saved.edit({ kind: 'connection', id: named('win-ad').id });
  if (name === 'treeeditsignin') {
    saved.edit(dbRef);
    setTimeout(() => clickTab('Sign-in'), 200);
  }
  if (name === 'treeeditrdpoptions') {
    saved.edit({ kind: 'connection', id: named('win-ad').id });
    setTimeout(() => clickTab('RDP options'), 200);
  }
  if (name === 'treenew') saved.newConnection();
  if (name === 'treenewinvalid') {
    saved.newConnection();
    setTimeout(() => {
      clickTab('Sign-in');
      document.querySelector<HTMLFormElement>('#connection-form')?.requestSubmit();
    }, 200);
  }
  if (name === 'treefolder') saved.edit({ kind: 'folder', id: folderNamed('Databases').id });
  if (name === 'treefoldersignin') {
    saved.edit({ kind: 'folder', id: folderNamed('Databases').id });
    setTimeout(() => clickTab('Sign-in'), 200);
  }
  if (name === 'treemove') saved.moving = { item: dbRef, name: db.name };
  if (name === 'treedrop')
    await saved.drop({ kind: 'connection', id: named('pi-hole').id }, folderNamed('Production').id);
  if (name === 'treeconnect') await saved.connect(named('win-ad').id);
  if (name === 'treetrash') await saved.trash({ kind: 'folder', id: folderNamed('Databases').id });
  if (name === 'treefoldersaved') saved.edit({ kind: 'folder', id: folderNamed('Staging').id });
  if (name.startsWith('treetrash') && name !== 'treetrash') {
    if (name !== 'treetrashnone') {
      await saved.trash({ kind: 'folder', id: folderNamed('Databases').id });
      await saved.trash({ kind: 'connection', id: named('nas').id });
    }
    saved.openTrash();
    if (name === 'treetrashconfirm' || name === 'treetrashunlock') {
      setTimeout(() => clickButton('Empty Trash'), 100);
    }
    if (name === 'treetrashunlock') setTimeout(() => clickButton('Empty Trash'), 200);
  }
  if (name === 'treeopen') await saved.connect(db.id);
  if (name === 'treesessions') {
    await saved.connect(db.id);
    await saved.connect(db.id);
    await saved.connect(named('win-ad').id);
    await saved.connect(named('pi-hole').id);
  }
  if (name === 'treefailed') await saved.connect(named('pi-hole').id);
  if (name === 'treepalette') shell.palette = 'commands';
  if (name === 'treevaultunlock' || name === 'treevaultwrong') {
    await saved.connect(named('db-02').id);
    const tab = workspace.tabs.at(-1);
    const wrong = { type: 'vault', master: 'wrong' } as const;
    if (name === 'treevaultwrong') setTimeout(() => tab && answerOpen(tab, wrong), 800);
  }
  if (name === 'treevaultmissing') await saved.connect(named('win-ad').id);
  if (name === 'treeeditsshconfig') saved.edit({ kind: 'connection', id: named('web-1').id });
  if (name === 'treevaultsaved') saved.edit({ kind: 'connection', id: named('db-02').id });
  if (name === 'treevaultinherited') saved.edit({ kind: 'connection', id: named('api-2').id });
  if (name === 'treevaultcreate') {
    saved.edit(dbRef);
    setTimeout(openEditorWithPassword, 100);
  }
}

const wrongPassword = { type: 'password', password: 'wrong', remember: false } as const;

function answerOpen(tab: SessionTab, reply: PromptReply) {
  if (tab.status.state === 'asking') tab.answer(tab.status.id, reply);
}

async function loadPasswordScene(name: string) {
  await saved.load();
  await saved.connect(named(name === 'passwordsaved' ? 'db-02' : 'db-01').id);
  const tab = workspace.tabs.at(-1);
  if (name === 'passwordwrong') setTimeout(() => tab && answerOpen(tab, wrongPassword), 1500);
}

export function loadScene(name: string | null) {
  if (name?.startsWith('tree')) {
    void loadTreeScene(name);
    return;
  }
  if (name?.startsWith('password')) {
    void loadPasswordScene(name);
    return;
  }
  if (name?.startsWith('files')) {
    shell.section = 'files';
    if (name === 'filesnone') return;
    if (name === 'filesended') {
      workspace.open(
        { protocol: 'ssh', host: 'fail-pi.lan', username: 'pi' },
        { name: 'pi-hole', env: 'none' },
      );
      return;
    }
    if (name === 'filesrdp') {
      workspace.open(
        { protocol: 'rdp', host: 'win-ad.corp', username: 'Administrator' },
        { name: 'win-ad', env: 'dev' },
      );
      return;
    }
    const tab = workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    const browser = fileBrowserOf(tab);
    const ready = setInterval(() => {
      if (tab.status.state !== 'connected' || !browser.listing) return;
      clearInterval(ready);
      if (name === 'filesdenied') void browser.open('/root');
      if (name === 'filesbig') void browser.open('/home/you/backups');
      if (name === 'filesempty') void browser.open('/home/you/empty');
      if (name === 'filestransfer') {
        const dump = browser.entries.find((entry) => entry.name.startsWith('dump'));
        if (dump) void browser.download(dump);
      }
      const named = (wanted: string) => browser.entries.find((entry) => entry.name === wanted)!;
      if (name === 'filesmenu') {
        browser.selected = named('backups').path;
        browser.menu = { entry: named('backups'), x: 120, y: 160 };
      }
      if (name === 'filesnewfolder') browser.startNewFolder();
      if (name === 'filesrename') browser.startRename(named('deploy.sh'));
      if (name === 'filesnametaken') {
        browser.startRename(named('deploy.sh'));
        browser.nameText = 'backups';
        void browser.saveName();
      }
      if (name === 'filesdelete') browser.startDelete(named('deploy.sh'));
      if (name === 'filesdeletefolder') browser.startDelete(named('backups'));
      if (name === 'filesdeletelink') browser.startDelete(named('logs'));
      if (name === 'filesdeletedenied') {
        browser.startDelete(named('backups'));
        void browser.confirmDelete();
      }
      if (name === 'filesdrop') {
        const box = document.querySelector('aside[aria-label="Files"]')!.getBoundingClientRect();
        const middle = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
        void emit('file-drag', { type: 'over', ...middle });
      }
      if (name === 'filesfolder') {
        browser.transfers = [
          {
            key: 'running',
            transfer: 'a',
            direction: 'down',
            name: 'backups',
            done: 9_000_000,
            total: 23_000_000,
            state: 'running',
          },
          {
            key: 'note',
            transfer: 'b',
            direction: 'up',
            name: 'site',
            done: 1,
            total: 1,
            state: 'finished',
            note: '2 links or special files were left out.',
          },
          {
            key: 'failed',
            transfer: 'c',
            direction: 'down',
            name: 'logs',
            done: 1,
            total: 1,
            state: 'failed',
            message: '2 of 40 items failed. First: No permission to use /home/you/logs/audit.',
          },
        ];
      }
    }, 50);
    return;
  }
  if (name?.startsWith('knownhosts')) {
    shell.section = 'knownhosts';
    if (name === 'knownhostsfilter') knownHosts.filter = 'db';
    if (name === 'knownhostsnomatch') knownHosts.filter = 'prod-db';
    if (name === 'knownhostsclosed') knownHosts.toggle('ssh_keys');
    if (name === 'knownhostshashed') knownHosts.filter = 'db-02';
    if (name === 'knownhostsremovehashed') {
      void knownHosts.load().then(() => {
        const section = knownHosts.sections[0];
        knownHosts.ask(
          section,
          section.entries.find((entry) => entry.known_as)!,
        );
      });
    }
    if (['knownhostsremove', 'knownhostscert', 'knownhostsgone', 'knownhostsdone'].includes(name)) {
      void knownHosts.load().then(() => {
        const section = knownHosts.sections[name === 'knownhostscert' ? 1 : 0];
        knownHosts.ask(section, section.entries[0]);
        if (name === 'knownhostsgone' || name === 'knownhostsdone') void knownHosts.confirm();
      });
    }
    return;
  }
  if (name === 'terminalmenu' || name === 'terminalmenucopy') {
    const tab = workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    const ready = setInterval(() => {
      const screen = document.querySelector('.xterm-screen');
      if (tab.status.state !== 'connected' || !screen) return;
      clearInterval(ready);
      setTimeout(() => openTerminalMenu(screen, name === 'terminalmenucopy'), 300);
    }, 50);
    return;
  }
  if (name === 'terminalfind' || name === 'terminalfindnone' || name === 'terminalfindregex') {
    const tab = workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    const ready = setInterval(() => {
      if (tab.status.state !== 'connected' || !document.querySelector('.xterm-screen')) return;
      clearInterval(ready);
      setTimeout(() => {
        tab.find.query = {
          terminalfind: 'postgres',
          terminalfindnone: 'mysql',
          terminalfindregex: 'pg_(',
        }[name];
        tab.find.regex = name === 'terminalfindregex';
        tab.find.open = true;
      }, 300);
    }, 50);
    return;
  }
  if (name === 'devtools') {
    shell.devToolsOpen = true;
    return;
  }
  if (name === 'reconnect') {
    const tab = workspace.open(
      { protocol: 'ssh', host: 'fail-pi.lan', username: 'pi' },
      { name: 'pi-hole', env: 'none' },
    );
    const ended = setInterval(() => {
      if (tab.status.state !== 'closed') return;
      clearInterval(ended);
      reconnect.start(tab);
    }, 50);
    return;
  }
  if (name === 'importask') {
    saved.askingImport = true;
    return;
  }
  if (name === 'importnotes') {
    void saved.importSshConfig();
    return;
  }
  if (name?.startsWith('vault')) {
    shell.section = 'vault';
    if (name === 'vaultchange' || name === 'vaultchangewrong') vault.changing = true;
    if (name === 'vaultchangewrong') setTimeout(submitWrongMaster, 100);
    if (name === 'vaultchanged') {
      setTimeout(() => void vault.changeMaster('old password', 'new password'), 100);
    }
    if (name === 'vaultkeysforget') {
      setTimeout(() => vault.keys[0] && vault.askForget(vault.keys[0]), 400);
    }
    if (name === 'vaultkeysforgotten') {
      setTimeout(() => {
        vault.askForget(vault.keys[0]);
        void vault.confirmForget();
      }, 400);
    }
    return;
  }
  if (name === 'hostkey') {
    workspace.open(
      { protocol: 'ssh', host: 'rekeyed.lan', port: 2222, username: 'ela' },
      { name: 'SSH test server', env: 'dev' },
    );
    return;
  }
  if (name === 'dropped') {
    const tab = workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    setTimeout(() => tab.cancel(), 1500);
    return;
  }
  if (name === 'connecting' || name === 'cancelled') {
    const tab = workspace.open(
      { protocol: 'ssh', host: 'held-db.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    if (name === 'cancelled') setTimeout(() => tab.cancel(), 600);
    return;
  }
  if (name === 'firstconnect' || name === 'hostkeynewtype' || name === 'hostkeyrevoked') {
    const host = { hostkeynewtype: 'newtype-host.lan', hostkeyrevoked: 'revoked-host.lan' }[
      name as 'hostkeynewtype'
    ];
    workspace.open(
      { protocol: 'ssh', host: host ?? 'new-host.lan', port: 2222, username: 'ela' },
      { name: 'SSH test server', env: 'dev' },
    );
    return;
  }
  if (name === 'passwordrdp') {
    workspace.open(
      { protocol: 'rdp', host: 'ask-win.corp', username: 'Administrator' },
      { name: 'win-ask', env: 'dev' },
    );
    return;
  }
  if (name === 'jumprdp') {
    workspace.open(
      { protocol: 'rdp', host: 'new-rdp.lan', username: 'ela', options: { proxy_jump: 'gate' } },
      { name: 'Build desktop', env: 'dev' },
    );
    return;
  }
  if (name?.startsWith('jump')) {
    const host =
      {
        jumptrust: 'new-db.lan',
        jumpchanged: 'rekeyed-db.lan',
        jumpfailed: 'fail-db.lan',
        jumpauth: 'denied-db.lan',
        jumpconnecting: 'held-db.lan',
        jumppassword: 'pw-db.lan',
        jumppasswordwrong: 'pw-db.lan',
        jumppasswordsaved: 'pwsaved-db.lan',
        jumppasswordaddress: 'pw-db.lan',
      }[name] ?? 'db.lan';
    const proxyJump = name === 'jumppasswordaddress' ? 'ops@10.0.0.9' : 'gate';
    const tab = workspace.open(
      { protocol: 'ssh', host, username: 'postgres', options: { proxy_jump: proxyJump } },
      { name: 'behind-gate', env: 'prod' },
    );
    if (name === 'jumppasswordwrong') setTimeout(() => answerOpen(tab, wrongPassword), 1500);
    return;
  }
  if (name?.startsWith('questions')) {
    const host = {
      questions: 'otp-db.lan',
      questionspassword: 'otppw-db.lan',
      questionsmany: 'otpmany-db.lan',
      questionsjump: 'otp-db.lan',
      questionsrefused: 'otpagain-db.lan',
    }[name];
    workspace.open(
      {
        protocol: 'ssh',
        host: host ?? 'otp-db.lan',
        username: 'postgres',
        options: name === 'questionsjump' ? { proxy_jump: 'gate' } : {},
      },
      { name: 'db-01', env: 'prod' },
    );
    return;
  }
  if (name?.startsWith('passphrase')) {
    const tab = workspace.open(
      {
        protocol: 'ssh',
        host: name === 'passphrasesaved' ? 'keysaved-db.lan' : 'key-db.lan',
        username: 'postgres',
        options: name === 'passphrasejump' ? { proxy_jump: 'gate' } : {},
      },
      { name: 'db-01', env: 'prod' },
    );
    if (name === 'passphrasenotsaved') {
      setTimeout(() => clickButton('Unlock the Vault'), 1500);
      setTimeout(() => void vault.unlock('master password').then(() => vault.answer(true)), 1800);
    }
    if (name === 'passphrasewrong') {
      const wrong = { type: 'passphrase', passphrase: 'wrong', remember: false } as const;
      setTimeout(() => answerOpen(tab, wrong), 1500);
    }
    return;
  }
  if (name === 'certchanged') {
    workspace.open(
      { protocol: 'rdp', host: 'rekeyed-rdp.lan', username: 'ela' },
      { name: 'RDP test desktop', env: 'dev' },
    );
    return;
  }
  if (name === 'released') {
    const tab = workspace.open(
      { protocol: 'rdp', host: 'win-ad.corp', username: 'Administrator' },
      { name: 'win-ad', env: 'dev' },
    );
    setTimeout(() => (tab.keyboardReleased = true), 1500);
    return;
  }
  if (name === 'shortcutssaved') {
    const tab = workspace.open(
      { protocol: 'rdp', host: 'win-ad.corp', username: 'Administrator' },
      { name: 'win-ad', env: 'dev' },
    );
    setTimeout(() => (tab.keyboardReleased = true), 1500);
    return;
  }
  if (name === 'shortcutsnotsaved') {
    workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    shell.sheetOpen = true;
    setTimeout(() => void shell.setHostKey('ScrollLock'), 300);
    return;
  }
  if (name?.startsWith('settings')) {
    workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    shell.openSettings(name === 'settingsvault' ? 'vault' : null);
    if (name === 'settingsnotsaved') setTimeout(() => terminalFont.setSize(16), 300);
    return;
  }
  if (name === 'shortcuts') {
    workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    shell.sheetOpen = true;
    return;
  }
  if (name === 'palette') {
    workspace.open(
      { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
      { name: 'db-01', env: 'prod' },
    );
    workspace.open(
      { protocol: 'rdp', host: 'win-ad.corp', username: 'Administrator' },
      { name: 'win-ad', env: 'dev' },
    );
    shell.palette = 'commands';
    return;
  }
  if (name === 'flood') {
    workspace.open(
      { protocol: 'bench-term', host: 'bench', options: { mb: 50 } },
      { name: 'Terminal flood', env: 'lab' },
    );
    return;
  }
  if (!name || !['busy', 'failure', 'desktop'].includes(name)) return;
  const open = workspace.open.bind(workspace);
  open(
    { protocol: 'ssh', host: 'db-01.internal', username: 'postgres' },
    { name: 'db-01', env: 'prod' },
  );
  open(
    { protocol: 'ssh', host: 'api-2.stg.internal', username: 'deploy' },
    { name: 'api-2', env: 'staging' },
  );
  open(
    { protocol: 'rdp', host: 'win-ad.corp', username: 'Administrator' },
    { name: 'win-ad', env: 'dev' },
  );
  open({ protocol: 'ssh', host: 'slow-nas.lan', username: 'admin' }, { name: 'nas', env: 'lab' });
  open({ protocol: 'ssh', host: 'fail-pi.lan', username: 'pi' }, { name: 'pi-hole', env: 'none' });
  workspace.activeKey = workspace.tabs[{ busy: 0, desktop: 2, failure: 4 }[name as 'busy']].key;
}

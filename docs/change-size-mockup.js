const commits = [
  { ref: 'main', title: 'Working changes', author: 'You', date: 'now', sha: 'WIP', files: 5, adds: 34, dels: 8, wip: true },
  { ref: '', title: 'Release notes now appear on every update', author: 'Jeremy', date: '1h ago', sha: '2652280', files: 5, adds: 42, dels: 9 },
  { ref: '', title: 'Beta releases stay separate from stable builds', author: 'Jeremy', date: '2h ago', sha: 'bbead82', files: 1, adds: 8, dels: 3 },
  { ref: '', title: 'New releases publish automatically', author: 'Jeremy', date: '2h ago', sha: '9b66730', files: 2, adds: 19, dels: 7 },
  { ref: 'release', title: 'Install GitWyrm silently for company-wide rollout', author: 'Jeremy', date: '7h ago', sha: '6facb74', files: 20, adds: 286, dels: 34 },
  { ref: '', title: 'Streamline the Windows install path', author: 'wutname1', date: '12h ago', sha: '1c74d85', files: 2, adds: 11, dels: 5 },
  { ref: '0.0.1', title: 'Readme now links directly to the download', author: 'wutname1', date: '12h ago', sha: 'ea2c9d0', files: 1, adds: 6, dels: 0, tag: true },
  { ref: '', title: 'Softer green accent and larger folder labels', author: 'Jeremy', date: '14h ago', sha: '6ce9cab', files: 54, adds: 842, dels: 418 },
  { ref: '', title: 'See and work on pull requests and issues', author: 'Jeremy', date: '15h ago', sha: '702c798', files: 20, adds: 397, dels: 72 },
  { ref: '', title: 'Open several repositories from the picker', author: 'Jeremy', date: '15h ago', sha: '2082453', files: 3, adds: 27, dels: 12 },
  { ref: '', title: 'Crash reports now include useful app details', author: 'Jeremy', date: '15h ago', sha: '1dbb908', files: 10, adds: 211, dels: 18 },
  { ref: '', title: 'Mark merge and conflict resolution as done', author: 'Jeremy', date: 'yesterday', sha: '291de63', files: 1, adds: 3, dels: 2 },
  { ref: '', title: 'Conflict screen now explains the next step', author: 'Jeremy', date: 'yesterday', sha: '451e591', files: 4, adds: 61, dels: 29 },
  { ref: '', title: 'Finish or cancel a paused rebase from the graph', author: 'Jeremy', date: 'yesterday', sha: '2318881', files: 3, adds: 38, dels: 14 },
  { ref: '', title: 'Pick a side of a conflict without leaving the file', author: 'Jeremy', date: 'yesterday', sha: 'a0ecea3', files: 5, adds: 122, dels: 96 },
  { ref: '', title: 'Switching tabs no longer shows a missing repository', author: 'Jeremy', date: 'yesterday', sha: '8e79272', files: 2, adds: 14, dels: 1 },
  { ref: '', title: 'Menus now show feedback while work is running', author: 'Jeremy', date: 'yesterday', sha: '5363b1b', files: 7, adds: 92, dels: 47 },
  { ref: '', title: 'Shared helpers keep dialogs consistent', author: 'Jeremy', date: 'yesterday', sha: '6376188', files: 7, adds: 183, dels: 121 },
];

let order = 'recent';
let toastTimer;

function scaleFor(commit) {
  const churn = commit.adds + commit.dels;
  const width = Math.max(8, Math.min(100, Math.round((Math.log10(churn + 1) / Math.log10(1300)) * 100)));
  const node = Math.max(10, Math.min(19, 9 + Math.round(Math.log10(churn + 1) * 3.2)));
  return { width, node };
}

function barHtml(commit, width = null) {
  const total = Math.max(1, commit.adds + commit.dels);
  const plus = Math.round((commit.adds / total) * 100);
  const minus = 100 - plus;
  const inline = width == null ? '' : ` style="width:${width}%"`;
  return `<span class="change-bar"${inline} aria-hidden="true"><i class="plus" style="width:${plus}%"></i><i class="minus" style="width:${minus}%"></i></span>`;
}

function rowHtml(commit, index, mode) {
  const scale = scaleFor(commit);
  const refs = commit.ref ? `<span class="ref-pill${commit.tag ? ' tag' : ''}">${commit.ref}</span>` : '';
  const stats = `<div class="stats-cell"><span class="file-count">${commit.files}</span><span class="numbers"><b class="adds">+${commit.adds}</b><b class="deletes">-${commit.dels}</b></span>${barHtml(commit, scale.width)}</div>`;
  const story = `<div class="message"><span class="message-title">${commit.title}</span><span class="story-changes" style="--size:${Math.max(18, scale.width)}px">${barHtml(commit)}<span class="files">${commit.files} ${commit.files === 1 ? 'file' : 'files'}</span><span class="numbers"><b class="adds">+${commit.adds}</b><b class="deletes">-${commit.dels}</b></span></span></div>`;
  return `<div class="commit-row${commit.wip ? ' wip' : ''}" role="button" tabindex="0" data-index="${index}" aria-label="${commit.title}, ${commit.adds} lines added and ${commit.dels} removed" style="--node:${scale.node}px">
    <div class="refs">${refs}</div>
    <div class="graph-cell"><i class="rail"></i><i class="node"></i></div>
    ${mode === 'column' ? `<div class="message">${commit.title}</div>` : story}
    <div class="author"><span class="avatar">${commit.author.slice(0, 1)}</span>${commit.author}</div>
    ${mode === 'column' ? stats : ''}
    <div class="date">${commit.date}</div>
    <div class="sha">${commit.sha}</div>
  </div>`;
}

function renderRows() {
  const rows = document.querySelector('.rows');
  const mode = document.body.dataset.mode;
  let visible = [...commits];
  if (order !== 'recent') {
    const wip = visible.shift();
    visible.sort((a, b) => (a.adds + a.dels - b.adds - b.dels) * (order === 'largest' ? -1 : 1));
    visible.unshift(wip);
  }
  rows.innerHTML = visible.map((commit, index) => rowHtml(commit, index, mode)).join('');
  rows.querySelectorAll('.commit-row').forEach((row) => {
    const activate = () => selectRow(row);
    row.addEventListener('click', activate);
    row.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        activate();
      }
    });
  });
}

function selectRow(row) {
  document.querySelectorAll('.commit-row.selected').forEach((item) => item.classList.remove('selected'));
  row.classList.add('selected');
  const label = row.querySelector('.message-title, .message').firstChild.textContent.trim();
  showToast(`<b>Selected</b> ${label}`);
}

function showToast(message) {
  const toast = document.querySelector('.toast');
  toast.innerHTML = message;
  toast.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove('show'), 1800);
}

document.addEventListener('DOMContentLoaded', () => {
  renderRows();

  document.querySelectorAll('.file').forEach((row) => {
    row.addEventListener('click', () => {
      document.querySelectorAll('.file.selected, .tree-row.selected').forEach((item) => item.classList.remove('selected'));
      row.classList.add('selected');
      showToast(`<b>Selected file</b> ${row.querySelector('.file-name')?.firstChild.textContent.trim() || 'Changed file'}`);
    });
  });

  document.querySelectorAll('.tree-row.folder').forEach((row) => {
    row.addEventListener('click', () => {
      const collapsed = row.classList.toggle('collapsed');
      row.querySelector('.twisty').textContent = collapsed ? '›' : '⌄';
      row.setAttribute('aria-expanded', String(!collapsed));
      showToast(`<b>${collapsed ? 'Closed' : 'Opened'} folder</b> ${row.querySelector('.tree-name').textContent.trim()}`);
    });
  });

  document.querySelectorAll('.tree-row.file-node').forEach((row) => {
    row.addEventListener('click', () => {
      document.querySelectorAll('.file.selected, .tree-row.selected').forEach((item) => item.classList.remove('selected'));
      row.classList.add('selected');
      showToast(`<b>Selected file</b> ${row.querySelector('.tree-name').textContent.trim()}`);
    });
  });

  const updateGroupCounts = () => {
    document.querySelectorAll('[data-group-header]').forEach((header) => {
      const group = header.dataset.groupHeader;
      const section = header.closest('.file-list, .tree');
      const count = section?.querySelector(`[data-file-group="${group}"]`)?.querySelectorAll('.file, .tree-row.file-node').length ?? 0;
      const target = header.querySelector('[data-count]');
      if (target) target.textContent = String(count);
    });
  };

  document.querySelectorAll('[data-stage-toggle]').forEach((button) => {
    button.addEventListener('click', (event) => {
      event.stopPropagation();
      if (button.classList.contains('pending')) return;
      const row = button.closest('.file, .tree-row.file-node');
      const name = button.dataset.fileName || row.querySelector('.file-name, .tree-name')?.textContent.trim() || 'File';
      const willStage = button.dataset.direction === 'stage';
      button.classList.add('pending');
      button.textContent = '·';
      button.setAttribute('aria-label', willStage ? `Staging ${name}` : `Unstaging ${name}`);

      setTimeout(() => {
        const nextDirection = willStage ? 'unstage' : 'stage';
        button.dataset.direction = nextDirection;
        button.textContent = willStage ? '-' : '+';
        button.setAttribute('aria-label', `${willStage ? 'Unstage' : 'Stage'} ${name}`);
        button.classList.remove('pending');
        row.classList.add('stage-flash');
        setTimeout(() => row.classList.remove('stage-flash'), 700);

        const groupRoot = row.closest('.file-list, .tree');
        const targetGroup = groupRoot?.querySelector(`[data-file-group="${willStage ? 'staged' : 'unstaged'}"]`);
        const existing = targetGroup
          ? [...targetGroup.querySelectorAll('.file, .file-node')].find((item) => item.dataset.filePath === row.dataset.filePath)
          : null;
        if (existing) {
          ['.adds', '.deletes'].forEach((selector) => {
            const targetStat = existing.querySelector(selector);
            const sourceStat = row.querySelector(selector);
            if (!targetStat || !sourceStat) return;
            const total = Number.parseInt(targetStat.textContent, 10) + Number.parseInt(sourceStat.textContent, 10);
            targetStat.textContent = total > 0 ? `+${total}` : String(total);
          });
          row.remove();
          existing.classList.add('stage-flash');
          setTimeout(() => existing.classList.remove('stage-flash'), 700);
        } else if (row.classList.contains('file')) {
          targetGroup?.appendChild(row);
        } else if (row.classList.contains('file-node') && targetGroup) {
          const pathTarget = [...targetGroup.querySelectorAll('[data-tree-files]')].find((item) => item.dataset.treeFiles === row.dataset.pathKey);
          pathTarget?.appendChild(row);
        }

        updateGroupCounts();

        showToast(`<b>${willStage ? 'Staged' : 'Unstaged'}</b> ${name}`);
      }, 360);
    });
  });

  document.querySelectorAll('.side-row').forEach((row) => {
    row.addEventListener('click', () => {
      document.querySelectorAll('.side-row.active').forEach((item) => item.classList.remove('active'));
      row.classList.add('active');
      showToast(`<b>Showing</b> ${row.textContent.trim().replace(/\d+$/, '')}`);
    });
  });

  document.querySelectorAll('.title-actions button, .branch-button, .commit-form button').forEach((button) => {
    button.addEventListener('click', () => showToast(`<b>${button.textContent.trim().split(/\s+/)[0]}</b> action previewed`));
  });

  document.querySelector('.size-key > button').addEventListener('click', () => {
    const key = document.querySelector('.size-key');
    const isOpen = key.classList.toggle('open');
    key.querySelector('button').setAttribute('aria-expanded', String(isOpen));
  });

  const sort = document.querySelector('[data-sort]');
  if (sort) {
    sort.addEventListener('click', () => {
      order = order === 'recent' ? 'largest' : order === 'largest' ? 'smallest' : 'recent';
      const labels = { recent: 'CHANGES', largest: 'CHANGES · BIGGEST', smallest: 'CHANGES · SMALLEST' };
      sort.querySelector('.sort-label').textContent = labels[order];
      sort.querySelector('.sort-arrow').textContent = order === 'recent' ? '↕' : order === 'largest' ? '↓' : '↑';
      renderRows();
      showToast(order === 'recent' ? '<b>Back to newest first</b>' : `<b>Sorted</b> ${order} changes first`);
    });
  }

  const appWindow = document.querySelector('.app-window');
  const syncDisplayOptions = () => {
    const linesOn = !appWindow.classList.contains('line-counts-off');
    const indicatorOn = !appWindow.classList.contains('indicator-off');
    const lineSetting = document.querySelector('[data-line-setting]');
    const indicatorSetting = document.querySelector('[data-indicator-setting]');
    lineSetting?.setAttribute('aria-pressed', String(linesOn));
    indicatorSetting?.setAttribute('aria-pressed', String(indicatorOn));
    const state = document.querySelector('.example-state');
    if (state) state.textContent = indicatorOn ? (linesOn ? 'Bars + totals' : 'Bars only') : 'Hidden';
    const messageHead = document.querySelector('[data-message-head]');
    if (messageHead) messageHead.textContent = indicatorOn ? 'COMMIT MESSAGE · SIZE' : 'COMMIT MESSAGE';
  };

  document.querySelector('[data-line-setting]')?.addEventListener('click', () => {
    const turningOn = appWindow.classList.contains('line-counts-off');
    appWindow.classList.toggle('line-counts-off', !turningOn);
    appWindow.classList.toggle('exact-off', !turningOn);
    syncDisplayOptions();
    showToast(`<b>Line counts ${turningOn ? 'shown' : 'hidden'}</b> Change bars stay visible`);
  });

  document.querySelector('[data-indicator-setting]')?.addEventListener('click', () => {
    const turningOn = appWindow.classList.contains('indicator-off');
    appWindow.classList.toggle('indicator-off', !turningOn);
    syncDisplayOptions();
    showToast(`<b>Change indicator ${turningOn ? 'shown' : 'hidden'}</b>`);
  });

  syncDisplayOptions();
});

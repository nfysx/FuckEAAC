/* =============================================================================
   FuckEAAC 前端逻辑
   - 所有后端交互都走 window.__TAURI__.core.invoke(命令名, 参数)
     （withGlobalTauri=true，所以不需要打包器、不需要 npm 依赖）
   - 想改界面行为->按钮绑定
   ============================================================================= */

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
let lastConclusion = '';   // 避免每 15 秒把同一句结论重复写进日志
let appInfo = null;        // 缓存的 app_info（含是否管理员）
let playTimer = null;      // 等待状态的轮询定时器
let playWasActive = false; // 上一轮是不是"正在等待"（用来只在结束那一刻打印日志）

/* ---------------------- 需要管理员时怎么提醒 ---------------------- */
/// 弹一个系统对话框问用户要不要切换管理员；返回 true 表示"用户同意，正在切换"
async function askElevate(reason) {
  try {
    const r = await invoke('plugin:dialog|message', {
      title: '需要管理员权限',
      message:
        `${reason}\n\n` +
        '当前是普通权限，无法停用/启动内核驱动与服务。\n' +
        '现在切换到管理员模式吗？ ',
      kind: 'warning',
      buttons: 'YesNo',
    });
    if (String(r).toLowerCase() === 'yes') {
      await invoke('relaunch_admin'); // 提权实例接管后，本进程会自动退出
      return true;
    }
  } catch (e) {
    logAdd('弹出提示失败: ' + e, 'WARN');
  }
  return false;
}

/// 做需要权限的操作之前的统一检查
async function ensureAdmin(action, driversOnDemand) {
  if (appInfo && appInfo.admin) return true;
  // 记住用户想做什么，提权重启后自动继续
  await invoke('set_pending', { action, driversOnDemand: !!driversOnDemand });
  const go = await askElevate(action === 'stop' ? '要进入游戏模式' : '要恢复代理工具');
  if (!go) {
    logAdd('已取消。可以右键 exe「以管理员身份运行」，或使用页面内「切换到管理员模式」按钮。', 'WARN');
  }
  return false;
}
const $ = (id) => document.getElementById(id);

/* ----------------------------- 日志 ----------------------------- */
function logAdd(line, level = 'INFO') {
  const box = $('log');
  const t = new Date().toLocaleTimeString('zh-CN', { hour12: false });
  const span = document.createElement('span');
  span.className = 'l-' + level;
  span.textContent = `[${t}] [${level}] ${line}\n`;
  box.appendChild(span);
  box.scrollTop = box.scrollHeight;
}
function logLines(lines) {
  (lines || []).forEach((l) => {
    let lv = 'INFO';
    if (/✓|成功|已停用|已恢复/.test(l)) lv = 'OK';
    else if (/✗|失败|无法|拒绝/.test(l)) lv = 'ERROR';
    else if (/^准备|^开始|^  /.test(l)) lv = 'STEP';
    else if (/提醒|注意|警告/.test(l)) lv = 'WARN';
    logAdd(l, lv);
  });
}

/* ----------------------------- 主题 ----------------------------- */
function applyTheme(dark) {
  document.body.classList.toggle('light', !dark);
  $('theme-label').textContent = dark ? '浅色' : '深色';
  localStorage.setItem('fuckeaac.theme', dark ? 'dark' : 'light');
}
function initTheme() {
  const saved = localStorage.getItem('fuckeaac.theme');
  applyTheme(saved ? saved === 'dark' : true);
}

/* ----------------------------- 渲染 ----------------------------- */
function badge(text, cls) {
  return `<span class="badge ${cls}">${text}</span>`;
}
function esc(s) {
  return String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

function renderInfo(info) {
  appInfo = info;
  // 权限徽标：管理员=绿色；普通权限=琥珀色且可点击（点一下就能切换）
  // 注意：变量名不叫 badge，免得遮蔽上面那个 badge() 小工具函数
  const el = $('admin-badge');
  if (info.admin) {
    el.textContent = '管理员模式';
    el.className = 'admin';
    el.onclick = null;
    el.title = '已具备停用内核驱动/服务所需的权限';
  } else {
    el.textContent = '⚠ 普通权限 · 点此切换';
    el.className = 'admin not-admin clickable';
    el.onclick = () => doAdmin();
    el.title = '未提权：无法停用内核驱动/服务。点击切换至管理员模式';
  }
  logAdd(`FuckEAAC ${info.version}（${info.backend}）  配置来源：${info.configSource}`, 'INFO');
  (info.bootLogs || []).forEach((l) => logAdd(l, 'INFO'));
  // 上次停了但没恢复（比如程序被强杀、断电重启）：提醒一句，state.json 还在，点恢复就能还原
  if (info.pendingRestore > 0) {
    logAdd(
      `上次游戏模式停用的 ${info.pendingRestore} 项还没恢复。` +
        '点左侧「恢复代理工具」即可还原。',
      'WARN'
    );
  }
}

function renderSnapshot(snap) {
  // 结论不再单独占一行（按要求去掉），改为写进日志
  if (snap.conclusion && snap.conclusion !== lastConclusion) {
    logAdd(snap.conclusion, snap.ready ? 'OK' : 'WARN');
    lastConclusion = snap.conclusion;
  }

  // 三张信息卡
  const cards = $('cards');
  cards.innerHTML = '';
  const groups = { Stop: [], Warn: [], Hint: [] };
  snap.targets.forEach((t) => (groups[t.effective] || groups.Warn).push(t));

  const mk = (title, rows) => {
    const el = document.createElement('section');
    el.className = 'card';
    el.innerHTML = `<div class="card-title">${title}</div>` + rows;
    return el;
  };
  const rowHtml = (k, v, cls = '', wide = false) =>
    `<div class="info-row${wide ? ' wide' : ''}"><span class="k">${esc(k)}</span><span class="v ${cls}">${esc(v)}</span></div>`;

  // 卡 1：环境与运行权限 + 系统代理 + 虚拟网卡
  // 虚拟网卡这一行同时显示"TUN 驱动服务是否在跑"—— 它是网卡枚举之外的第二重证据
  const tun = snap.tun_adapters.length
    ? snap.tun_adapters.map((a) => `${a.name} [${a.status}]`).join('，') +
      (snap.tun_driver ? `　（TUN 驱动 ${snap.tun_driver} 运行中）` : '')
    : snap.tun_driver
      ? `无匹配网卡，但 TUN 驱动 ${snap.tun_driver} 正在运行`
      : '未发现';
  cards.appendChild(
    mk(
      '本机状态',
      rowHtml('运行权限', snap.admin ? '管理员' : '普通用户', snap.admin ? 'ok' : 'warn') +
        rowHtml('系统代理', snap.proxy_enabled ? `已开启 ${snap.proxy_server}` : '未开启', snap.proxy_enabled ? 'warn' : 'ok') +
        rowHtml('虚拟网卡', tun, snap.tun_adapters.length ? 'warn' : 'ok', true) +
        rowHtml('本工具会自动停用', `${groups.Stop.length} 项`, '') +
        rowHtml(
          '需你手动处理',
          `${groups.Hint.length} 项`,
          groups.Hint.length ? 'warn' : 'ok'
        )
    )
  );

  // 卡 2：会停用 / 只提醒 的清单（各取前几条，避免卡片过长）
  // 说明：Action=Hint 的目标（内核反作弊那类）**正在运行时归到"需停用"栏**，
  //       但明确标注"需你手动" —— 工具只负责提醒，不动手。
  const top = (arr, n) =>
    arr.slice(0, n).map((t) => rowHtml(t.name, t.running ? '运行中' : '未运行', t.running ? 'warn' : 'ok')).join('') ||
    '<div class="info-row"><span class="k">（无）</span></div>';
  const hintRows = groups.Hint.map((t) => t.name);
  cards.appendChild(
    mk(
      '本次会停用',
      top(groups.Stop, 6) +
        (hintRows.length
          ? `<div class="info-row wide"><span class="k">需你手动处理</span>` +
            `<span class="v warn">${esc(hintRows.join('、'))}` +
            `（工具不会动：请自行关闭，或重启电脑）</span></div>`
          : '')
    )
  );
  cards.appendChild(mk('仅提醒', top(groups.Warn, 6)));

  // 检测结果表
  const body = $('result-body');
  body.innerHTML = '';
  if (!snap.targets.length) {
    body.innerHTML = '<tr><td colspan="5" class="empty">未检测到相关软件。</td></tr>';
  }
  snap.targets.forEach((t) => {
    const hits = [];
    if (t.processes.length) hits.push('进程: ' + t.processes.join(', '));
    t.services.forEach((s) => hits.push(`${s.name}[${s.status}]`));
    const how =
      t.effective === 'Stop'
        ? badge('需停用', 'stop')
        : t.effective === 'Hint'
          ? badge('需你手动', 'stop')
          : badge('仅提醒', 'warn');
    const tr = document.createElement('tr');
    tr.innerHTML =
      `<td>${how}</td>` +
      `<td>${esc(t.name)}</td>` +
      `<td class="mono">${esc(hits.join('  |  '))}</td>` +
      `<td>${t.running ? badge('运行中', 'run') : badge('未运行', 'warn')}</td>` +
      `<td>${esc(t.note || '')}</td>`;
    body.appendChild(tr);
  });
}

/* ----------------------------- 动作 ----------------------------- */
async function refresh() {
  try {
    const r = await invoke('get_status');
    renderSnapshot(r.snapshot);
  } catch (e) {
    logAdd('刷新失败: ' + e, 'ERROR');
  }
}

async function doStop(andPlay) {
  const demand = $('chk-demand').checked;
  if (!(await ensureAdmin('stop', demand))) return;   // 未提权：先提示切换
  logAdd(andPlay ? '=== 进入游戏模式 ===' : '=== 仅停用代理 ===', 'STEP');
  try {
    const r = await invoke('stop_targets', { driversOnDemand: demand });
    logLines(r.logs);
    if (r.needAdmin) {
      logAdd('需要管理员权限：点右上角「⚠ 普通权限」或左侧「切换到管理员模式」。', 'WARN');
    } else if (r.snapshot) {
      renderSnapshot(r.snapshot);
    }
    if (andPlay && r.ok && !r.needAdmin) {
      if ($('chk-wait').checked) {
        await startPlayWatch();
      } else {
        logAdd('此次仅停用 记得点「恢复代理工具」。', 'INFO');
      }
    }
  } catch (e) {
    logAdd('停用失败: ' + e, 'ERROR');
  }
}

/* --------------------- 游戏模式：后台等游戏进程 --------------------- */
/// 要等哪些进程：优先用户选的 exe；留空就用配置里的 GameProcessNames
async function watchNames() {
  const names = [];
  const typed = $('txt-game').value.trim();
  if (typed) names.push(typed);
  if (!names.length) {
    try {
      const c = await invoke('get_config');
      (c.config.GameProcessNames || []).forEach((n) => names.push(n));
    } catch (e) {
      /* 下面 start_play 会给出更明确的提示 */
    }
  }
  return names;
}

function fmtDur(sec) {
  const s = Math.max(0, sec | 0);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h) return `${h} 小时 ${m} 分`;
  if (m) return `${m} 分 ${s % 60} 秒`;
  return `${s} 秒`;
}

async function startPlayWatch() {
  const names = await watchNames();
  const mins = parseInt($('num-timeout').value, 10) || 720;
  try {
    const r = await invoke('start_play', { processes: names, timeoutMinutes: mins });
    if (!r.ok) {
      logAdd('没能开始等待：' + r.error, 'WARN');
      return;
    }
    logAdd(
      `开始等待游戏进程：${names.join(' / ') || '（配置里的游戏进程名）'}。\n` +
        '  本工具不会代启动 EA App / 游戏。\n' +
        `  游戏退出后会自动恢复；等超过 ${mins} 分钟也会自动恢复。\n` ,
      'STEP'
    );
    playWasActive = true;
    renderPlay(r.status);
    startPolling();
  } catch (e) {
    logAdd('开始等待失败: ' + e, 'ERROR');
  }
}

/// 结束等待（不恢复 —— 恢复由「恢复代理工具」明确执行）
async function stopWatching() {
  try {
    await invoke('cancel_play');
  } catch (e) {
    /* 忽略 */
  }
  if (playTimer) {
    clearInterval(playTimer);
    playTimer = null;
  }
  playWasActive = false;
  renderPlay({ active: false, phase: 'canceled' });
}

function renderPlay(st) {
  const box = $('play-banner');
  const st2 = st || {};
  if (st2.active) {
    box.classList.remove('hidden');
    box.classList.remove('warn');
    const what = st2.phase === 'in-game' ? '游戏运行中，等待退出' : '等你启动游戏';
    $('play-text').textContent =
      `正在等待：${st2.process} · ${what} · 已等 ${fmtDur(st2.elapsedSecs)}（${st2.started} 开始）`;
    return;
  }
  if (st2.phase === 'gave-up') {
    // 没等到游戏 → 一直挂着提醒（代理还是停用状态），直到你恢复
    box.classList.remove('hidden');
    box.classList.add('warn');
    $('play-text').textContent = '没等到游戏进程，已停止等待 —— 代理工具仍是停用状态，记得恢复。';
    return;
  }
  box.classList.add('hidden');
  box.classList.remove('warn');
}

function startPolling() {
  if (playTimer) return;
  playTimer = setInterval(pollPlay, 3000);
  pollPlay();
}

async function pollPlay() {
  let st;
  try {
    st = (await invoke('play_status')).status || {};
  } catch (e) {
    return; // 轮询失败不刷屏，下次再试
  }
  // 结束的那一刻：把后台攒的日志一次性打到界面上
  if (playWasActive && !st.active) {
    logAdd('=== 等待结束 ===', 'STEP');
    (st.logs || []).forEach((l) => {
      const bad = /✗|失败|没等到|取消/.test(l);
      logAdd(l, bad ? 'WARN' : /✓|已启动|已恢复|已退出/.test(l) ? 'OK' : 'INFO');
    });
    const lv = st.phase === 'done' ? 'OK' : 'WARN';
    logAdd('结果：' + (st.note || ''), lv);
    refresh(); // 恢复完刷新一下卡片
  }
  playWasActive = !!st.active;
  renderPlay(st);
  if (!st.active && playTimer) {
    clearInterval(playTimer);
    playTimer = null;
  }
}

async function doRestore() {
  if (!(await ensureAdmin('restore', false))) return;   // 未提权：先提示切换
  await stopWatching();                                  // 先让后台别再等了，避免两边抢
  logAdd('=== 恢复代理工具 ===', 'STEP');
  try {
    const r = await invoke('restore_targets');
    logLines(r.logs);
    if (r.needAdmin) logAdd('需要管理员权限：点右上角「⚠ 普通权限」或左侧「切换到管理员模式」。', 'WARN');
    else if (r.snapshot) renderSnapshot(r.snapshot);
  } catch (e) {
    logAdd('恢复失败: ' + e, 'ERROR');
  }
}

async function doDrill() {
  logAdd('=== 测试 ===', 'STEP');
  const on = !(await invoke('set_dry_run', { on: true })).dryRun;
  await invoke('set_dry_run', { on });
  logAdd(on ? '测试模式已开启：所有停用/恢复仅打印。' : '测试模式已关闭。', 'OK');
  await doStop(false);
}

async function doDiag() {
  logAdd('=== 环境诊断 ===', 'STEP');
  try {
    const r = await invoke('run_diag');
    (r.checks || []).forEach((c) => {
      const lv = c.state === 'ok' ? 'OK' : c.state === 'bad' ? 'ERROR' : c.state === 'warn' ? 'WARN' : 'INFO';
      logAdd(`${c.name}: ${c.value}${c.note ? '  — ' + c.note : ''}`, lv);
    });
  } catch (e) {
    logAdd('诊断失败: ' + e, 'ERROR');
  }
}

async function doBrowse() {
  try {
    const p = await invoke('plugin:dialog|open', {
      options: {
        multiple: false,
        directory: false,
        filters: [{ name: '可执行文件', extensions: ['exe'] }],
      },
    });
    if (p) {
      $('txt-game').value = typeof p === 'string' ? p : p.path || '';
      logAdd('已选择游戏主程序: ' + $('txt-game').value, 'OK');
    }
  } catch (e) {
    logAdd('选择文件失败: ' + e, 'WARN');
  }
}

async function doRescan() {
  logAdd('=== 重新扫描本机，生成配置 ===', 'STEP');
  try {
    const r = await invoke('rescan_config');
    logLines(r.logs);
    logAdd('配置已写入：' + r.path, 'OK');
    await refresh();
  } catch (e) {
    logAdd('扫描失败: ' + e, 'ERROR');
  }
}

async function doExportConfig() {
  try {
    const r = await invoke('export_config');
    logAdd('配置已导出: ' + r.path, 'OK');
    await invoke('open_path', { path: r.path });
  } catch (e) {
    logAdd('导出配置失败: ' + e, 'ERROR');
  }
}

async function doOpenLog() {
  const info = await invoke('app_info');
  await invoke('open_path', { path: info.stateDir + '\\fuckeaac.log' });
}

async function doAdmin() {
  logAdd('正在切换到管理员模式。', 'STEP');
  const r = await invoke('relaunch_admin');
  logLines(r.logs);
}

/* ------------------ 点 X 关窗：问一句"留在后台还是彻底退出" ------------------ */
/// 后端拦下关窗后会发来 `app://close-requested`，这里负责弹提示并决定去向。
async function onCloseRequested() {
  let exit = false;
  try {
    const r = await invoke('plugin:dialog|message', {
      title: '转入后台托管',
      message:
        'FuckEAAC 会留在右下角托盘继续运行\n\n' +
        '「是」= 彻底退出程序\n' +
        '「否」= 留在后台',
      kind: 'info',
      buttons: 'YesNo',
    });
    exit = String(r).toLowerCase() === 'yes';
  } catch (e) {
    /* 弹窗失败就当"留在后台"，绝不让窗口卡在关不掉的状态 */
  }
  if (exit) {
    await invoke('quit_app');
  } else {
    await invoke('hide_window');
    logAdd('已转入后台托管：程序仍在托盘运行，双击托盘图标可重新打开窗口。', 'INFO');
  }
}

async function doQuit() {
  // 还在等游戏的时候退出 → 问一句，别让代理工具莫名其妙一直是关着的
  if (playWasActive) {
    let wantRestore = false;
    try {
      const r = await invoke('plugin:dialog|message', {
        title: '还在等游戏',
        message:
          '现在还在等游戏进程。退出前要先恢复代理工具吗？\n\n' +
          '「是」= 先恢复再退出\n' +
          '「否」= 直接退出',
        kind: 'warning',
        buttons: 'YesNo',
      });
      wantRestore = String(r).toLowerCase() === 'yes';
    } catch (e) {
      /* 弹窗失败就当"否" */
    }
    if (wantRestore) {
      await stopWatching();
      try {
        logLines((await invoke('restore_targets')).logs);
      } catch (e) {
        logAdd('恢复失败: ' + e, 'ERROR');
      }
    } else {
      await invoke('cancel_play');
    }
  }
  await invoke('quit_app');
}

/* ----------------------------- 按钮绑定 ----------------------------- */
function bind() {
  $('btn-theme').onclick = () => applyTheme(document.body.classList.contains('light')); // 浅→深
  $('btn-diag').onclick = doDiag;
  $('btn-github').onclick = () => {
    window.open('https://github.com/nfysx/FuckEAAC', '_blank');
  };
  $('btn-play').onclick = () => doStop(true);
  $('btn-stop').onclick = () => doStop(false);
  $('btn-restore').onclick = doRestore;
  $('btn-drill').onclick = doDrill;
  $('btn-refresh').onclick = () => { logAdd('手动刷新状态…', 'STEP'); refresh(); };
  $('btn-rescan').onclick = doRescan;
  $('btn-config').onclick = doExportConfig;
  $('btn-log').onclick = doOpenLog;
  $('btn-quit').onclick = doQuit;
  $('btn-browse').onclick = doBrowse;
  $('btn-play-cancel').onclick = doRestore;   // 横幅上的「停止等待并立即恢复」

  // 托盘菜单 + 关窗 → 前端动作（保证 UI 逻辑只有一处）
  window.__TAURI__.event.listen('tray://play', () => doStop(true));
  window.__TAURI__.event.listen('tray://restore', () => doRestore());
  window.__TAURI__.event.listen('tray://diag', () => doDiag());
  window.__TAURI__.event.listen('app://close-requested', onCloseRequested);
}

/* ----------------------------- 启动 ----------------------------- */
(async function start() {
  initTheme();
  bind();
  logAdd('界面已加载，正在读取状态…', 'INFO');
  try {
    renderInfo(await invoke('app_info'));
    await refresh();
    // 万一后端已经有等待在跑（正常不会：等待线程随进程结束），把横幅和轮询接上
    const st0 = (await invoke('play_status')).status;
    if (st0 && st0.active) {
      playWasActive = true;
      renderPlay(st0);
      startPolling();
    }
    // 之前因为权限被中断的操作：提权重启后自动继续
    const pend = await invoke('take_pending');
    if (pend && pend.pending) {
      const a = pend.pending.action;
      logAdd('检测到切换管理员前未完成的操作，将自动继续：' + a, 'STEP');
      if (a === 'stop') {
        if (pend.pending.driversOnDemand) $('chk-demand').checked = true;
        await doStop(true);
      } else if (a === 'restore') {
        await doRestore();
      }
    }
  } catch (e) {
    logAdd('初始化失败: ' + e, 'ERROR');
  }
  // 每 15 秒自动刷新一次状态（改完配置/关了软件后能自己更新）
  setInterval(refresh, 15000);
})();

// Strange Room 界面逻辑。
//
// 刻意用原生 JS：界面只有四个状态（首页 / 等待 / 传输中 / 结果），
// 引入构建链和框架的收益远小于它带来的复杂度。等界面真的复杂到
// 需要框架时再换，而不是一开始就假设它需要。

const T = window.__TAURI__ || {};
const invoke = (cmd, args) => {
  const fn = T.core?.invoke || T.invoke;
  if (!fn) throw new Error("Tauri API 不可用（应该只在客户端里运行）");
  return fn(cmd, args);
};

const $ = (id) => document.getElementById(id);
const show = (id) => {
  document.querySelectorAll(".screen").forEach((s) => s.classList.remove("active"));
  $(id).classList.add("active");
};

const human = (n) => {
  const u = ["B", "KB", "MB", "GB", "TB"];
  let v = n, i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return i === 0 ? `${n} B` : `${v.toFixed(1)} ${u[i]}`;
};

// ── 首页：收集要分享的路径 ────────────────────────────────
let paths = [];

function renderPaths() {
  const ul = $("paths");
  ul.innerHTML = "";
  paths.forEach((p, i) => {
    const li = document.createElement("li");
    const name = p.split(/[\/]/).filter(Boolean).pop() || p;
    li.innerHTML = `<span title="${p}">${name}</span>`;
    const x = document.createElement("button");
    x.className = "x"; x.textContent = "移除";
    x.onclick = () => { paths.splice(i, 1); renderPaths(); };
    li.appendChild(x);
    ul.appendChild(li);
  });
  $("start-share").disabled = paths.length === 0;
}

function addPaths(list) {
  for (const p of list) {
    if (p && !paths.includes(p)) paths.push(p);
  }
  renderPaths();
}

$("add-path").onclick = () => {
  const v = $("path-input").value.trim();
  if (v) { addPaths([v]); $("path-input").value = ""; }
};
$("path-input").addEventListener("keydown", (e) => {
  if (e.key === "Enter") $("add-path").click();
});
$("clear-paths").onclick = () => { paths = []; renderPaths(); };

// 拖拽：这个产品的核心动作是"把东西摊到桌上"，所以拖拽必须能用
const wv = T.webview?.getCurrentWebview?.();
const dropZone = $("drop");
if (wv?.onDragDropEvent) {
  wv.onDragDropEvent((ev) => {
    const p = ev.payload || {};
    if (p.type === "over" || p.type === "enter") dropZone.classList.add("hot");
    else if (p.type === "drop") { dropZone.classList.remove("hot"); addPaths(p.paths || []); }
    else dropZone.classList.remove("hot");
  });
}

// ── 开始分享 ──────────────────────────────────────────────
$("start-share").onclick = async () => {
  const btn = $("start-share");
  btn.disabled = true;
  btn.textContent = "正在扫描文件…";
  try {
    const info = await invoke("start_share", { paths });
    $("qr").innerHTML = info.qr_svg;
    $("share-summary").textContent = `${info.summary}`;
    $("share-addrs").textContent = (info.addresses || []).join("　");
    $("copy-payload").dataset.payload = info.payload;
    show("screen-share");
  } catch (e) {
    showResult("没能开始分享", String(e), "如果是大文件夹，扫描需要一点时间；路径错误也会失败。");
  } finally {
    btn.disabled = false;
    btn.textContent = "开始分享";
  }
};

$("copy-payload").onclick = async () => {
  const text = $("copy-payload").dataset.payload || "";
  const btn = $("copy-payload");
  try {
    await navigator.clipboard.writeText(text);
    btn.textContent = "已复制";
  } catch {
    // 剪贴板不可用时退化成让用户手选
    btn.textContent = "请手动复制下方连接串";
    $("share-addrs").textContent = text;
  }
  setTimeout(() => (btn.textContent = "复制连接串"), 1600);
};

$("cancel-share").onclick = async () => {
  await invoke("cancel_share");
  show("screen-home");
};

// ── 开始接收 ──────────────────────────────────────────────
$("start-receive").onclick = async () => {
  const payload = $("payload-input").value.trim();
  const dest = $("dest-input").value.trim() || ".";
  const hint = $("receive-hint");
  if (!payload) { hint.textContent = "请先粘贴连接串"; return; }
  try {
    const info = await invoke("inspect_payload", { payload });
    hint.textContent = `正在连接 ${info.host} …`;
  } catch (e) {
    hint.textContent = String(e);
    return;
  }
  showTransfer();
  try {
    await invoke("start_receive", { payload, dest });
  } catch (e) {
    showResult("没能开始接收", String(e), "");
  }
};

// 诊断网络：连不上时先跑这个，而不是让用户自己猜
$("diagnose").onclick = async () => {
  const payload = $("payload-input").value.trim();
  if (!payload) { $("receive-hint").textContent = "请先粘贴连接串，再诊断"; return; }
  const btn = $("diagnose");
  btn.disabled = true;
  btn.textContent = "自检中…";
  try {
    const report = await invoke("diagnose_payload", { payload });
    showResult("网络自检", report, "");
  } catch (e) {
    showResult("自检没能完成", String(e), "请检查连接串是否完整。");
  } finally {
    btn.disabled = false;
    btn.textContent = "诊断网络";
  }
};

// ── 传输中的进度显示 ──────────────────────────────────────
let startedAt = 0;
let lastDone = 0;
let lastAt = 0;

function showTransfer() {
  $("cur-file").textContent = "";
  $("file-bar").style.width = "0%";
  $("overall-bar").style.width = "0%";
  $("file-stats").textContent = "";
  $("file-eta").textContent = "";
  $("overall-stats").textContent = "";
  $("peer").textContent = "正在连接…";
  const cancelBtn = $("cancel-transfer");
  if (cancelBtn) { cancelBtn.disabled = false; cancelBtn.textContent = "停止接收"; }
  startedAt = Date.now();
  show("screen-transfer");
}

function showResult(title, body, hint) {
  $("result-title").innerHTML = `<span class="big">${title}</span>`;
  $("result-body").textContent = body || "";
  $("result-hint").textContent = hint || "";
  show("screen-result");
}

$("back-home").onclick = () => { paths = []; renderPaths(); show("screen-home"); };
// 停止接收：走内核的协作式取消，已下载的部分会保留，下次能续传
$("cancel-transfer").onclick = async () => {
  const btn = $("cancel-transfer");
  btn.disabled = true;
  btn.textContent = "正在停止…";
  try { await invoke("cancel_transfer"); } catch (e) { btn.textContent = "停止接收"; }
  // 真正的界面切换等后端发来事件，避免"点了却看不出发生了什么"
};

// 接收内核事件
const listen = T.event?.listen;
if (listen) {
  listen("transfer", (ev) => {
    const e = ev.payload || {};
    switch (e.kind) {
      case "peerConnected":
        $("peer").textContent = `${e.peer} 已连上 · 共 ${e.totalFiles} 个文件（${human(e.totalBytes)}）`;
        showTransfer();
        break;
      case "fileStarted":
        $("cur-file").textContent =
          e.resumedFrom > 0 ? `${e.path}（从 ${human(e.resumedFrom)} 续传）` : e.path;
        $("file-bar").style.width = "0%";
        $("file-stats").textContent = `0 / ${human(e.size)}`;
        break;
      case "progress": {
        const pct = e.total > 0 ? (e.done / e.total) * 100 : 0;
        $("file-bar").style.width = `${pct.toFixed(1)}%`;
        $("file-stats").textContent = `${human(e.done)} / ${human(e.total)}`;
        const opct = e.overallTotal > 0 ? (e.overallDone / e.overallTotal) * 100 : 0;
        $("overall-bar").style.width = `${opct.toFixed(1)}%`;
        $("overall-stats").textContent = `${human(e.overallDone)} / ${human(e.overallTotal)} · ${opct.toFixed(0)}%`;
        // 速度与剩余时间：用两次采样之间的增量估算
        const now = Date.now();
        if (now - lastAt > 400) {
          const rate = lastAt ? (e.overallDone - lastDone) / ((now - lastAt) / 1000) : 0;
          if (rate > 0 && e.overallTotal > e.overallDone) {
            const left = (e.overallTotal - e.overallDone) / rate;
            $("file-eta").textContent =
              `${human(rate)}/s · 剩 ${left > 60 ? Math.round(left / 60) + " 分" : Math.round(left) + " 秒"}`;
          }
          lastDone = e.overallDone; lastAt = now;
        }
        break;
      }
      case "done":
        showResult(
          "传输完成",
          `${e.files} 个文件，共 ${e.humanBytes}`,
          ""
        );
        break;
      case "failed": {
        // 用户主动停止不是失败，别用"没能完成"吓人，也别让他以为进度白丢了
        const cancelled = String(e.message || "").includes("已取消");
        if (cancelled) {
          showResult(
            "已停止",
            "已接收的部分已经保留下来了。下次连接同一个分享，会自动从断点接着传。",
            ""
          );
        } else {
          showResult(
            "没能完成",
            e.message,
            "如果提示指纹不匹配，说明二维码已过期或被改动，请让对方重新出示二维码。"
          );
        }
        break;
      }
      case "warn":
        $("peer").textContent += ` · ${e.message}`;
        break;
    }
  });
}

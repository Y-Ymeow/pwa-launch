/**
 * UI components (browser UI, verify assist button)
 */

// 浏览器 UI - 用于外部链接跳转
export function injectBrowserUI() {
  if (document.getElementById("pwa-browser-ui")) return;

  const ui = document.createElement("div");
  ui.id = "pwa-browser-ui";
  ui.innerHTML = `
    <div style="
      position: fixed;
      top: 0;
      left: 0;
      right: 0;
      height: 48px;
      background: #1a1a2e;
      display: flex;
      align-items: center;
      padding: 0 12px;
      z-index: 2147483647;
      box-shadow: 0 2px 8px rgba(0,0,0,0.3);
    ">
      <button id="pwa-back-btn" style="
        background: rgba(255,255,255,0.1);
        border: none;
        color: white;
        padding: 8px 16px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 14px;
        margin-right: 12px;
      ">← 返回</button>
      <div id="pwa-url-bar" style="
        flex: 1;
        background: rgba(255,255,255,0.1);
        border-radius: 4px;
        padding: 8px 12px;
        color: rgba(255,255,255,0.8);
        font-size: 13px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      ">${location.href}</div>
    </div>
    <div style="height: 48px;"></div>
  `;

  document.body.insertBefore(ui, document.body.firstChild);

  document.getElementById("pwa-back-btn").onclick = () => {
    if (window.__TAURI_BRIDGE__?.navigateBack) {
      window.__TAURI_BRIDGE__.navigateBack();
    } else {
      history.back();
    }
  };
}

// 验证助手按钮
export function createVerifyAssistButton(bridge) {
  if (document.getElementById("pwa-verify-assist-btn")) return;

  const btn = document.createElement("button");
  btn.id = "pwa-verify-assist-btn";
  btn.innerHTML = "✓ 验证完成";
  btn.style.cssText = `
    position: fixed !important;
    bottom: 20px !important;
    right: 20px !important;
    z-index: 2147483647 !important;
    padding: 12px 24px !important;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important;
    color: white !important;
    border: none !important;
    border-radius: 8px !important;
    font-size: 14px !important;
    font-weight: 600 !important;
    cursor: pointer !important;
    box-shadow: 0 4px 15px rgba(0,0,0,0.3) !important;
    transition: all 0.3s ease !important;
  `;

  btn.onclick = async () => {
    btn.innerHTML = "⟳ 同步中...";
    btn.style.background = "linear-gradient(135deg, #11998e 0%, #38ef7d 100%) !important";

    try {
      const currentUrl = location.href;
      await bridge.invoke("sync_webview_cookies", { url: currentUrl });

      btn.innerHTML = "✓ 同步完成";
      btn.style.background = "linear-gradient(135deg, #11998e 0%, #38ef7d 100%) !important";

      setTimeout(() => {
        btn.innerHTML = "✓ 验证完成";
        btn.style.background = "linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important";
      }, 2000);
    } catch (err) {
      console.error("[PWA Adapt] Failed to sync cookies:", err);
      btn.innerHTML = "✗ 失败";
      btn.style.background = "linear-gradient(135deg, #eb3349 0%, #f45c43 100%) !important";

      setTimeout(() => {
        btn.innerHTML = "✓ 验证完成";
        btn.style.background = "linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important";
      }, 2000);
    }
  };

  setTimeout(() => {
    if (document.body) {
      document.body.appendChild(btn);
    }
  }, 1000);
}

export function shouldShowVerifyButton() {
  const title = document.title.toLowerCase();
  const bodyText = document.body?.textContent?.toLowerCase() || "";

  const verifyKeywords = [
    "just a moment",
    "checking your browser",
    "captcha",
    "验证码",
    "安全验证",
    "人机验证",
    "cloudflare",
    "please wait",
  ];

  return verifyKeywords.some((kw) => title.includes(kw) || bodyText.includes(kw));
}

export function initVerifyAssist(bridge) {
  if (shouldShowVerifyButton()) {
    createVerifyAssistButton(bridge);
    return;
  }

  setTimeout(() => {
    if (shouldShowVerifyButton()) {
      createVerifyAssistButton(bridge);
    }
  }, 3000);
}

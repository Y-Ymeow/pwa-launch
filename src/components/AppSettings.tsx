import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AppSettingsProps {
  show: boolean;
  onClose: () => void;
  showMessage: (type: "success" | "error", text: string) => void;
}

const PRESET_USER_AGENTS = [
  {
    name: "使用系统默认 (推荐)",
    value: "",
  },
  {
    name: "Android Chrome",
    value: "Mozilla/5.0 (Linux; Android 13; TECNO BG6 Build/TP1A.220624.014) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.7632.159 Mobile Safari/537.36",
  },
  {
    name: "iPhone Safari",
    value: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
  },
  {
    name: "Windows Chrome",
    value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
  },
  {
    name: "macOS Safari",
    value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
  },
];

export function AppSettings({ show, onClose, showMessage }: AppSettingsProps) {
  const [activeTab, setActiveTab] = useState<"general" | "network">("general");

  // User-Agent 设置（默认为空，使用系统默认）
  const [userAgent, setUserAgent] = useState("");
  // 屏幕常亮设置
  const [keepScreenOn, setKeepScreenOn] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (show) {
      loadSettings();
    }
  }, [show]);

  const loadSettings = async () => {
    try {
      const uaResult = await invoke<{ success: boolean; data: string | null }>("get_app_config", { key: "user_agent" });
      if (uaResult.success && uaResult.data) {
        setUserAgent(uaResult.data);
      }
      const screenResult = await invoke<{ success: boolean; data: boolean | string | null }>("get_app_config", { key: "keep_screen_on" });
      if (screenResult.success && screenResult.data !== null) {
        // 处理字符串或布尔值
        const val = screenResult.data;
        setKeepScreenOn(typeof val === 'boolean' ? val : val === 'true');
      }
    } catch (error) {
      console.error("Failed to load settings:", error);
    }
  };

  const saveSettings = async () => {
    setLoading(true);
    try {
      await invoke("set_app_config", { key: "user_agent", value: userAgent });
      await invoke("set_app_config", { key: "keep_screen_on", value: keepScreenOn });
      // 调用原生命令设置屏幕常亮
      await invoke("set_keep_screen_on", { enabled: keepScreenOn });
      showMessage("success", "设置已保存");
      onClose();
    } catch (error) {
      showMessage("error", `保存失败: ${String(error)}`);
    } finally {
      setLoading(false);
    }
  };

  if (!show) return null;

  return (
    <div
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        background: "rgba(0,0,0,0.7)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 10000,
      }}
      onClick={onClose}
    >
      <div
        style={{
          background: "#1a1a2e",
          borderRadius: "16px",
          width: "90%",
          maxWidth: "700px",
          maxHeight: "85vh",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* 头部 */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "20px 24px",
            borderBottom: "1px solid rgba(255,255,255,0.1)",
          }}
        >
          <h3 style={{ margin: 0, color: "white", fontSize: "18px" }}>⚙️ 应用设置</h3>
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              color: "rgba(255,255,255,0.6)",
              cursor: "pointer",
              fontSize: "20px",
            }}
          >
            ✕
          </button>
        </div>

        {/* 标签页 */}
        <div
          style={{
            display: "flex",
            borderBottom: "1px solid rgba(255,255,255,0.1)",
            background: "rgba(0,0,0,0.2)",
          }}
        >
          <button
            onClick={() => setActiveTab("general")}
            style={{
              padding: "12px 24px",
              background: activeTab === "general" ? "rgba(102,126,234,0.3)" : "transparent",
              border: "none",
              borderBottom: activeTab === "general" ? "2px solid #667eea" : "none",
              color: activeTab === "general" ? "white" : "rgba(255,255,255,0.6)",
              cursor: "pointer",
              fontSize: "14px",
            }}
          >
            常规
          </button>
          <button
            onClick={() => setActiveTab("network")}
            style={{
              padding: "12px 24px",
              background: activeTab === "network" ? "rgba(102,126,234,0.3)" : "transparent",
              border: "none",
              borderBottom: activeTab === "network" ? "2px solid #667eea" : "none",
              color: activeTab === "network" ? "white" : "rgba(255,255,255,0.6)",
              cursor: "pointer",
              fontSize: "14px",
            }}
          >
            网络
          </button>
        </div>

        {/* 内容区 */}
        <div style={{ padding: "24px", overflow: "auto", flex: 1 }}>
          {activeTab === "general" && (
            <div>
              <h4 style={{ color: "white", margin: "0 0 16px 0", fontSize: "16px" }}>
                🌐 User-Agent 设置
              </h4>
              
              <div style={{ marginBottom: "16px" }}>
                <label style={{ color: "rgba(255,255,255,0.8)", fontSize: "14px" }}>
                  选择预设：
                </label>
                <select
                  value={PRESET_USER_AGENTS.find((p) => p.value === userAgent)?.name || ""}
                  onChange={(e) => {
                    const preset = PRESET_USER_AGENTS.find((p) => p.name === e.target.value);
                    if (preset) {
                      setUserAgent(preset.value);
                    }
                  }}
                  style={{
                    width: "100%",
                    padding: "10px",
                    marginTop: "8px",
                    borderRadius: "8px",
                    border: "1px solid rgba(255,255,255,0.2)",
                    background: "rgba(0,0,0,0.3)",
                    color: "white",
                    fontSize: "14px",
                  }}
                >
                  <option value="">自定义</option>
                  {PRESET_USER_AGENTS.map((preset) => (
                    <option key={preset.name} value={preset.name}>
                      {preset.name}
                    </option>
                  ))}
                </select>
              </div>

              <div style={{ marginBottom: "20px" }}>
                <label style={{ color: "rgba(255,255,255,0.8)", fontSize: "14px" }}>
                  自定义 User-Agent：
                </label>
                <textarea
                  value={userAgent}
                  onChange={(e) => setUserAgent(e.target.value)}
                  rows={4}
                  style={{
                    width: "100%",
                    padding: "10px",
                    marginTop: "8px",
                    borderRadius: "8px",
                    border: "1px solid rgba(255,255,255,0.2)",
                    background: "rgba(0,0,0,0.3)",
                    color: "white",
                    fontSize: "13px",
                    fontFamily: "monospace",
                    resize: "vertical",
                  }}
                />
              </div>

              <div
                style={{
                  padding: "12px",
                  background: "rgba(255,255,255,0.05)",
                  borderRadius: "8px",
                }}
              >
                <p style={{ margin: 0, fontSize: "12px", color: "rgba(255,255,255,0.6)", lineHeight: 1.5 }}>
                  💡 User-Agent 会用于所有代理请求。某些网站会根据 User-Agent 返回不同的内容或进行限制。
                </p>
              </div>

              <div style={{ marginTop: "24px", paddingTop: "24px", borderTop: "1px solid rgba(255,255,255,0.1)" }}>
                <h4 style={{ color: "white", margin: "0 0 16px 0", fontSize: "16px" }}>
                  📱 屏幕常亮
                </h4>
                
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "16px",
                    background: "rgba(255,255,255,0.05)",
                    borderRadius: "8px",
                  }}
                >
                  <div>
                    <div style={{ color: "white", fontSize: "14px", marginBottom: "4px" }}>
                      保持屏幕常亮
                    </div>
                    <div style={{ color: "rgba(255,255,255,0.6)", fontSize: "12px" }}>
                      开启后屏幕将不会自动熄灭（适用于阅读、观看视频等场景）
                    </div>
                  </div>
                  <button
                    onClick={() => setKeepScreenOn(!keepScreenOn)}
                    style={{
                      width: "48px",
                      height: "26px",
                      borderRadius: "13px",
                      border: "none",
                      background: keepScreenOn ? "#38ef7d" : "rgba(255,255,255,0.2)",
                      cursor: "pointer",
                      position: "relative",
                      transition: "background 0.3s",
                    }}
                  >
                    <div
                      style={{
                        width: "22px",
                        height: "22px",
                        borderRadius: "50%",
                        background: "white",
                        position: "absolute",
                        top: "2px",
                        left: keepScreenOn ? "24px" : "2px",
                        transition: "left 0.3s",
                      }}
                    />
                  </button>
                </div>
              </div>
            </div>
          )}

          {activeTab === "network" && (
            <div>
              <h4 style={{ color: "white", margin: "0 0 16px 0", fontSize: "16px" }}>
                🌐 网络设置
              </h4>
              <p style={{ color: "rgba(255,255,255,0.6)", fontSize: "14px" }}>
                代理设置请使用主界面的 🔧 按钮。
              </p>
              
              <div
                style={{
                  padding: "16px",
                  background: "rgba(255,255,255,0.05)",
                  borderRadius: "8px",
                  marginTop: "16px",
                }}
              >
                <h5 style={{ color: "white", margin: "0 0 8px 0" }}>本地代理服务器</h5>
                <p style={{ margin: 0, fontSize: "13px", color: "rgba(255,255,255,0.6)", lineHeight: 1.5 }}>
                  地址: http://localhost:19315/api/proxy?url=...<br />
                  用于解决 CORS 跨域问题，自动处理 cookies。
                </p>
              </div>
            </div>
          )}
        </div>

        {/* 底部按钮 */}
        <div
          style={{
            display: "flex",
            gap: "10px",
            justifyContent: "flex-end",
            padding: "16px 24px",
            borderTop: "1px solid rgba(255,255,255,0.1)",
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "10px 20px",
              borderRadius: "8px",
              border: "1px solid rgba(255,255,255,0.2)",
              background: "transparent",
              color: "white",
              cursor: "pointer",
            }}
          >
            取消
          </button>
          <button
            onClick={saveSettings}
            disabled={loading}
            style={{
              padding: "10px 20px",
              borderRadius: "8px",
              border: "none",
              background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
              color: "white",
              cursor: loading ? "not-allowed" : "pointer",
              opacity: loading ? 0.7 : 1,
            }}
          >
            {loading ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}

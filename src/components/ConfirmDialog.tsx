import { useState, useCallback } from "react";
import "./ConfirmDialog.css";

interface ConfirmOptions {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  isDanger?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  isOpen: boolean;
  resolve: ((value: boolean) => void) | null;
}

// 全局状态管理
let showConfirmFn: ((options: ConfirmOptions) => Promise<boolean>) | null = null;

export function useConfirm() {
  return {
    confirm: useCallback((options: ConfirmOptions) => {
      if (showConfirmFn) {
        return showConfirmFn(options);
      }
      return Promise.resolve(false);
    }, []),
  };
}

export function ConfirmDialogProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<ConfirmState>({
    isOpen: false,
    title: "",
    message: "",
    confirmText: "确定",
    cancelText: "取消",
    isDanger: false,
    resolve: null,
  });

  const showConfirm = useCallback((options: ConfirmOptions): Promise<boolean> => {
    return new Promise((resolve) => {
      setState({
        ...options,
        confirmText: options.confirmText || "确定",
        cancelText: options.cancelText || "取消",
        isOpen: true,
        resolve,
      });
    });
  }, []);

  // 注册全局函数
  showConfirmFn = showConfirm;

  const handleConfirm = () => {
    if (state.resolve) {
      state.resolve(true);
    }
    setState((prev) => ({ ...prev, isOpen: false }));
  };

  const handleCancel = () => {
    if (state.resolve) {
      state.resolve(false);
    }
    setState((prev) => ({ ...prev, isOpen: false }));
  };

  return (
    <>
      {children}
      {state.isOpen && (
        <div className="confirm-overlay" onClick={handleCancel}>
          <div className="confirm-dialog" onClick={(e) => e.stopPropagation()}>
            <h3 className="confirm-title">{state.title}</h3>
            <p className="confirm-message">{state.message}</p>
            <div className="confirm-buttons">
              <button className="confirm-btn cancel" onClick={handleCancel}>
                {state.cancelText}
              </button>
              <button
                className={`confirm-btn confirm ${state.isDanger ? "danger" : ""}`}
                onClick={handleConfirm}
              >
                {state.confirmText}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

// 全局 confirm 函数
export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  if (showConfirmFn) {
    return showConfirmFn(options);
  }
  return Promise.resolve(false);
}

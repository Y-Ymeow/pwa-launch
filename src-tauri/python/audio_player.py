#!/usr/bin/env python3
"""
跨平台音频播放器
支持 Linux 和 Android（通过 Termux 或 Pydroid）
"""

import sys
import json
import subprocess
import time
import threading
from pathlib import Path

class AudioPlayer:
    def __init__(self):
        self.process = None
        self.current_url = None
        self.start_time = None
        self.is_playing = False
        self.is_paused = False
        self.duration_ms = 0
        
    def play(self, url):
        """播放音频"""
        self.stop()
        
        self.current_url = url
        self.start_time = time.time()
        self.is_playing = True
        self.is_paused = False
        
        # 使用 mpv 后台播放
        cmd = [
            "mpv",
            "--no-video",
            "--no-terminal",
            "--force-window=no",
            "--really-quiet",
            "--idle=no",
            url
        ]
        
        try:
            self.process = subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )
            return {"success": True, "message": "Playing"}
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    def pause(self):
        """暂停（mpv 不支持标准暂停，只能停止）"""
        self.is_paused = True
        return {"success": True}
    
    def resume(self):
        """继续"""
        self.is_paused = False
        return {"success": True}
    
    def stop(self):
        """停止"""
        if self.process:
            self.process.terminate()
            self.process.wait()
            self.process = None
        
        self.is_playing = False
        self.is_paused = False
        self.current_url = None
        return {"success": True}
    
    def get_state(self):
        """获取播放状态"""
        if not self.is_playing:
            return {
                "currentUrl": "",
                "positionMs": 0,
                "durationMs": 0,
                "isPlaying": False,
                "isPaused": False
            }
        
        elapsed_ms = int((time.time() - self.start_time) * 1000) if self.start_time else 0
        
        # 检查进程是否还在运行
        if self.process and self.process.poll() is not None:
            self.is_playing = False
        
        return {
            "currentUrl": self.current_url or "",
            "positionMs": elapsed_ms,
            "durationMs": self.duration_ms,
            "isPlaying": self.is_playing and not self.is_paused,
            "isPaused": self.is_paused
        }
    
    def set_volume(self, volume):
        """设置音量 (0.0 - 1.0)"""
        # mpv 命令行不支持运行时调整音量
        return {"success": False, "message": "Volume control not supported"}

# 全局播放器实例
player = AudioPlayer()

def main():
    """主函数 - 读取 JSON 命令从 stdin"""
    print("Audio Player Ready", flush=True)
    
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        
        try:
            cmd = json.loads(line)
            action = cmd.get("action")
            
            if action == "play":
                result = player.play(cmd.get("url"))
            elif action == "pause":
                result = player.pause()
            elif action == "resume":
                result = player.resume()
            elif action == "stop":
                result = player.stop()
            elif action == "getState":
                result = player.get_state()
            elif action == "setVolume":
                result = player.set_volume(cmd.get("volume", 1.0))
            else:
                result = {"error": f"Unknown action: {action}"}
            
            print(json.dumps(result), flush=True)
            
        except json.JSONDecodeError:
            print(json.dumps({"error": "Invalid JSON"}), flush=True)
        except Exception as e:
            print(json.dumps({"error": str(e)}), flush=True)

if __name__ == "__main__":
    main()

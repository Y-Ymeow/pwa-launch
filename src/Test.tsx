import { convertFileSrc } from "@tauri-apps/api/core";
import { audioDir } from "@tauri-apps/api/path";
import React from "react";

import { readFile, BaseDirectory } from "@tauri-apps/plugin-fs";

export default function Test() {
  const [url, setUrl] = React.useState("");
  const [imageUrl, setImageUrl] = React.useState("");

  // 定义在组件外部或使用 useRef 确保单例
  let audioCtx: any = null;
  let source: any = null;

  async function playAudioLowLevel(path) {
    try {
      // 1. 初始化 AudioContext (单例)
      if (!audioCtx) {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)();
      }

      // 2. 停止之前的播放
      if (source) {
        source.stop();
      }

      // 3. 读取并解码
      const contents = await readFile(path);
      // 注意：decodeAudioData 会消耗掉原有的 ArrayBuffer，所以用 slice() 备份一下
      const buffer = await audioCtx.decodeAudioData(contents.buffer.slice(0));

      // 4. 创建播放节点
      source = audioCtx.createBufferSource();
      source.buffer = buffer;
      source.connect(audioCtx.destination);

      // 5. 开始播放
      source.start(0);
      console.log("Web Audio API 播放启动成功");
    } catch (err) {
      console.error("解码或播放失败:", err);
    }
  }
  React.useEffect(() => {
    setUrl(convertFileSrc("/computer/Music/李茂珍 - 에피소드.mp3"));

    setImageUrl(convertFileSrc("/computer/Music/.김한겸 - SHINING_cover.jpg"));

    fetch("static://localhost//etc/hosts");

    audioDir().then((dir) => {
      console.log(dir);
    });
  });

  return (
    <div>
      <audio src={url} controls></audio>
      <button
        onClick={async () =>
          await playAudioLowLevel(
            convertFileSrc("/computer/Music/李茂珍 - 에피소드.mp3"),
          )
        }
      >
        as,djasjkdhaskjda
      </button>
      <img src={imageUrl} alt="test" />
    </div>
  );
}

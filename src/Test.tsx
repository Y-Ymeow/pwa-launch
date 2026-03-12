import { convertFileSrc } from "@tauri-apps/api/core";
import { audioDir } from "@tauri-apps/api/path";
import React from "react";

export default function Test() {
  const [url, setUrl] = React.useState("");
  const [imageUrl, setImageUrl] = React.useState("");

  React.useEffect(() => {
    setUrl(
      "http://127.0.0.1:8765/?file=" +
        encodeURI("/home/ymeow/音乐/李茂珍 - 에피소드.mp3"),
    );

    setImageUrl(
      "http://127.0.0.1:8765/?file=" +
        encodeURI("/home/ymeow/音乐/.김한겸 - SHINING_cover.jpg"),
    );

    fetch("static://localhost//etc/hosts");

    audioDir().then((dir) => {
      console.log(dir);
    });
  });

  return (
    <div>
      <audio src={url} controls></audio>

      <img src={imageUrl} alt="test" />
    </div>
  );
}

const COMMANDS: &[&str] = &[
  "play",
  "pause",
  "resume",
  "stop",
  "set_volume",
  "seek",
  "set_loop",
  "get_state",
  "get_position",
  "get_duration",
  "get_current_url",
];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .ios_path("ios")
    .build();
}
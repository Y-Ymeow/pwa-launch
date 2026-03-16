# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# 保留 JNI 调用的类（Rust 通过 JNI 调用）
-keep class com.pwa.container.AudioPlayerBridge {
    public static *;
}

# 保留 AudioPlayer 相关类
-keep class com.pwa.container.AudioPlayer { *; }
-keep class com.pwa.container.AudioPlayerInstance { *; }

# 保留 ExoPlayer 相关类（防止被混淆导致播放失败）
-keep class androidx.media3.** { *; }
-dontwarn androidx.media3.**

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
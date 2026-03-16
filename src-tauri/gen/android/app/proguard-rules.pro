# ProGuard rules for PWA Container

# 保留所有 com.pwa.container 包下的类
-keep class com.pwa.container.** { *; }

# 保留 Tauri 相关类
-keep class app.tauri.** { *; }

# 保留插件类
-keep class com.plugin.** { *; }

# 保留 Kotlin 元数据
-keepattributes *Annotation*, Signature, Exceptions, InnerClasses, EnclosingMethod, KotlinMetadata

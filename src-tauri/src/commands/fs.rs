use std::fs;
use std::path::PathBuf;
use crate::models::CommandResponse;
use base64::Engine;

#[cfg(target_os = "android")]
use jni::signature::JavaType;
#[cfg(target_os = "android")]
use jni::objects::JString;
#[cfg(target_os = "android")]
use std::convert::TryFrom;

/// 读取目录内容
#[tauri::command]
pub async fn fs_read_dir(path: String) -> Result<CommandResponse<Vec<FsEntry>>, String> {
    let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            result.push(FsEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }
    }
    
    Ok(CommandResponse::success(result))
}

/// 写入文件 (支持 Base64 内容)
#[tauri::command]
pub async fn fs_write_file(path: String, content: String, is_binary: bool) -> Result<CommandResponse<bool>, String> {
    let data = if is_binary {
        base64::engine::general_purpose::STANDARD.decode(content).map_err(|e| e.to_string())?
    } else {
        content.into_bytes()
    };
    
    fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(CommandResponse::success(true))
}

/// 创建目录
#[tauri::command]
pub async fn fs_create_dir(path: String, recursive: bool) -> Result<CommandResponse<bool>, String> {
    if recursive {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
    } else {
        fs::create_dir(path).map_err(|e| e.to_string())?;
    }
    Ok(CommandResponse::success(true))
}

/// 删除文件或目录
#[tauri::command]
pub async fn fs_remove(path: String, recursive: bool) -> Result<CommandResponse<bool>, String> {
    let path_buf = PathBuf::from(&path);
    if path_buf.is_dir() {
        if recursive {
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_dir(path).map_err(|e| e.to_string())?;
        }
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(CommandResponse::success(true))
}

/// 检查路径是否存在
#[tauri::command]
pub async fn fs_exists(path: String) -> Result<CommandResponse<bool>, String> {
    Ok(CommandResponse::success(PathBuf::from(path).exists()))
}

#[derive(serde::Serialize)]
pub struct FsEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
}

/// Android 存储权限检查结果
#[derive(serde::Serialize)]
pub struct StoragePermissionStatus {
    pub granted: bool,
    pub can_request: bool,
}

/// 检查 Android 存储权限状态
#[tauri::command]
pub async fn check_storage_permission() -> Result<CommandResponse<StoragePermissionStatus>, String> {
    #[cfg(target_os = "android")]
    {
        // 通过 JNI 检查权限
        match check_manage_storage_permission_jni() {
            Ok(granted) => Ok(CommandResponse::success(StoragePermissionStatus {
                granted,
                can_request: !granted,
            })),
            Err(e) => Err(format!("检查权限失败: {}", e)),
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        // 非 Android 平台默认返回已授权
        Ok(CommandResponse::success(StoragePermissionStatus {
            granted: true,
            can_request: false,
        }))
    }
}

/// 打开 Android 设置页面让用户授权
#[tauri::command]
pub async fn request_storage_permission() -> Result<CommandResponse<bool>, String> {
    #[cfg(target_os = "android")]
    {
        // 打开应用设置页面
        match open_storage_settings_jni() {
            Ok(_) => Ok(CommandResponse::success(true)),
            Err(e) => Err(format!("打开设置失败: {}", e)),
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(CommandResponse::success(true))
    }
}

#[cfg(target_os = "android")]
fn check_manage_storage_permission_jni() -> Result<bool, String> {
    let vm = unsafe { jni::JavaVM::from_raw(ndk_context::android_context().vm().cast()) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    
    // 调用 Environment.isExternalStorageManager()
    let env_cls = env.find_class("android/os/Environment").map_err(|e| e.to_string())?;
    let granted = env.call_static_method(
        &env_cls,
        "isExternalStorageManager",
        "()Z",
        &[],
    ).map_err(|e| e.to_string())?.z().map_err(|e| e.to_string())?;
    
    Ok(granted)
}

#[cfg(target_os = "android")]
fn open_storage_settings_jni() -> Result<(), String> {
    let vm = unsafe { jni::JavaVM::from_raw(ndk_context::android_context().vm().cast()) }
        .map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    
    let ctx = ndk_context::android_context().context();
    let ctx_ref = unsafe { jni::objects::JObject::from_raw(ctx as *mut jni::sys::jobject) };
    
    // 创建 Intent
    let intent_cls = env.find_class("android/content/Intent").map_err(|e| e.to_string())?;
    let intent = env.new_object(
        &intent_cls,
        "()V",
        &[],
    ).map_err(|e| e.to_string())?;
    
    // 设置 Action: android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION
    let action = env.get_static_field(
        "android/provider/Settings",
        "ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION",
        "Ljava/lang/String;",
    ).map_err(|e| e.to_string())?.l().map_err(|e| e.to_string())?;
    
    env.call_method(
        &intent,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[(&action).into()],
    ).map_err(|e| e.to_string())?;
    
    // 添加包名 URI
    let package_name = env.call_method(
        &ctx_ref,
        "getPackageName",
        "()Ljava/lang/String;",
        &[],
    ).map_err(|e| e.to_string())?.l().map_err(|e| e.to_string())?;
    
    let uri_cls = env.find_class("android/net/Uri").map_err(|e| e.to_string())?;
    
    // 拼接字符串
    let package_name_str: JString = package_name.into();
    let package_name_rust = env.get_string(&package_name_str).map_err(|e| e.to_string())?;
    let uri_data = format!("package:{}", package_name_rust.to_str().unwrap_or(""));
    let uri_data_jstring = env.new_string(&uri_data).map_err(|e| e.to_string())?;
    
    let uri = env.call_static_method(
        &uri_cls,
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[(&uri_data_jstring).into()],
    ).map_err(|e| e.to_string())?.l().map_err(|e| e.to_string())?;
    
    env.call_method(
        &intent,
        "setData",
        "(Landroid/net/Uri;)Landroid/content/Intent;",
        &[(&uri).into()],
    ).map_err(|e| e.to_string())?;
    
    // 启动设置页面
    env.call_method(
        &ctx_ref,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[(&intent).into()],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

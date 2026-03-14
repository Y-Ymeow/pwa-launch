package com.pwa.container

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.content.ActivityNotFoundException
import android.content.DialogInterface
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.view.View
import android.webkit.*
import android.widget.EditText
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultCallback
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import java.io.File
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.*

/**
 * 支持视频全屏的 WebChromeClient
 * 复制 RustWebChromeClient 的功能并添加视频全屏支持
 */
class FullscreenChromeClient(
    private val activity: WryActivity,
    private val mainActivity: MainActivity
) : WebChromeClient() {

    private interface PermissionListener {
        fun onPermissionSelect(isGranted: Boolean?)
    }

    private interface ActivityResultListener {
        fun onActivityResult(result: ActivityResult?)
    }

    private var permissionListener: PermissionListener? = null
    private var activityListener: ActivityResultListener? = null

    // 使用 MainActivity 中已注册的 launchers
    private val permissionLauncher: ActivityResultLauncher<Array<String>>
        get() = mainActivity.permissionLauncher
    private val activityLauncher: ActivityResultLauncher<Intent>
        get() = mainActivity.activityLauncher

    init {
        // 重新设置回调
        val permissionCallback = ActivityResultCallback { isGranted: Map<String, Boolean> ->
            if (permissionListener != null) {
                var granted = true
                for ((_, value) in isGranted) {
                    if (!value) granted = false
                }
                permissionListener!!.onPermissionSelect(granted)
            }
        }
        val activityCallback = ActivityResultCallback { result: ActivityResult? ->
            if (activityListener != null) {
                activityListener!!.onActivityResult(result)
            }
        }
    }

    // ========== 视频全屏支持 ==========
    override fun onShowCustomView(view: View, callback: CustomViewCallback) {
        mainActivity.enterVideoFullscreen(view, callback)
    }

    override fun onHideCustomView() {
        mainActivity.exitVideoFullscreen()
    }

    override fun onShowCustomView(
        view: View,
        requestedOrientation: Int,
        callback: CustomViewCallback
    ) {
        onShowCustomView(view, callback)
    }

    // ========== 权限处理 ==========
    override fun onPermissionRequest(request: PermissionRequest) {
        val isRequestPermissionRequired = Build.VERSION.SDK_INT >= Build.VERSION_CODES.M
        val permissionList: MutableList<String> = ArrayList()
        if (listOf(*request.resources).contains("android.webkit.resource.VIDEO_CAPTURE")) {
            permissionList.add(Manifest.permission.CAMERA)
        }
        if (listOf(*request.resources).contains("android.webkit.resource.AUDIO_CAPTURE")) {
            permissionList.add(Manifest.permission.MODIFY_AUDIO_SETTINGS)
            permissionList.add(Manifest.permission.RECORD_AUDIO)
        }
        if (permissionList.isNotEmpty() && isRequestPermissionRequired) {
            val permissions = permissionList.toTypedArray()
            permissionListener = object : PermissionListener {
                override fun onPermissionSelect(isGranted: Boolean?) {
                    if (isGranted == true) {
                        request.grant(request.resources)
                    } else {
                        request.deny()
                    }
                }
            }
            permissionLauncher.launch(permissions)
        } else {
            request.grant(request.resources)
        }
    }

    // ========== JS 对话框 ==========
    override fun onJsAlert(view: WebView, url: String, message: String, result: JsResult): Boolean {
        if (activity.isFinishing) {
            return true
        }
        val builder = AlertDialog.Builder(view.context)
        builder
            .setMessage(message)
            .setPositiveButton("OK") { dialog: DialogInterface, _: Int ->
                dialog.dismiss()
                result.confirm()
            }
            .setOnCancelListener { dialog: DialogInterface ->
                dialog.dismiss()
                result.cancel()
            }
        val dialog = builder.create()
        dialog.show()
        return true
    }

    override fun onJsConfirm(view: WebView, url: String, message: String, result: JsResult): Boolean {
        if (activity.isFinishing) {
            return true
        }
        val builder = AlertDialog.Builder(view.context)
        builder
            .setMessage(message)
            .setPositiveButton("OK") { dialog: DialogInterface, _: Int ->
                dialog.dismiss()
                result.confirm()
            }
            .setNegativeButton("Cancel") { dialog: DialogInterface, _: Int ->
                dialog.dismiss()
                result.cancel()
            }
            .setOnCancelListener { dialog: DialogInterface ->
                dialog.dismiss()
                result.cancel()
            }
        val dialog = builder.create()
        dialog.show()
        return true
    }

    override fun onJsPrompt(
        view: WebView,
        url: String,
        message: String,
        defaultValue: String,
        result: JsPromptResult
    ): Boolean {
        if (activity.isFinishing) {
            return true
        }
        val builder = AlertDialog.Builder(view.context)
        val input = EditText(view.context)
        builder
            .setMessage(message)
            .setView(input)
            .setPositiveButton("OK") { dialog: DialogInterface, _: Int ->
                dialog.dismiss()
                val inputText = input.text.toString().trim()
                result.confirm(inputText)
            }
            .setNegativeButton("Cancel") { dialog: DialogInterface, _: Int ->
                dialog.dismiss()
                result.cancel()
            }
            .setOnCancelListener { dialog: DialogInterface ->
                dialog.dismiss()
                result.cancel()
            }
        val dialog = builder.create()
        dialog.show()
        return true
    }

    // ========== 地理位置权限 ==========
    override fun onGeolocationPermissionsShowPrompt(
        origin: String,
        callback: GeolocationPermissions.Callback
    ) {
        super.onGeolocationPermissionsShowPrompt(origin, callback)
        val geoPermissions =
            arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION)
        if (!hasPermissions(activity, geoPermissions)) {
            permissionListener = object : PermissionListener {
                override fun onPermissionSelect(isGranted: Boolean?) {
                    if (isGranted == true) {
                        callback.invoke(origin, true, false)
                    } else {
                        val coarsePermission = arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION)
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
                            hasPermissions(activity, coarsePermission)
                        ) {
                            callback.invoke(origin, true, false)
                        } else {
                            callback.invoke(origin, false, false)
                        }
                    }
                }
            }
            permissionLauncher.launch(geoPermissions)
        } else {
            callback.invoke(origin, true, false)
        }
    }

    private fun hasPermissions(activity: Activity, permissions: Array<String>): Boolean {
        for (permission in permissions) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                if (activity.checkSelfPermission(permission) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
                    return false
                }
            }
        }
        return true
    }

    // ========== 文件选择器 ==========
    override fun onShowFileChooser(
        webView: WebView,
        filePathCallback: ValueCallback<Array<Uri?>?>,
        fileChooserParams: FileChooserParams
    ): Boolean {
        val acceptTypes = listOf(*fileChooserParams.acceptTypes)
        val captureEnabled = fileChooserParams.isCaptureEnabled
        val capturePhoto = captureEnabled && acceptTypes.contains("image/*")
        val captureVideo = captureEnabled && acceptTypes.contains("video/*")
        if (capturePhoto || captureVideo) {
            if (isMediaCaptureSupported()) {
                showMediaCaptureOrFilePicker(filePathCallback, fileChooserParams, captureVideo)
            } else {
                permissionListener = object : PermissionListener {
                    override fun onPermissionSelect(isGranted: Boolean?) {
                        if (isGranted == true) {
                            showMediaCaptureOrFilePicker(filePathCallback, fileChooserParams, captureVideo)
                        } else {
                            filePathCallback.onReceiveValue(null)
                        }
                    }
                }
                val camPermission = arrayOf(Manifest.permission.CAMERA)
                permissionLauncher.launch(camPermission)
            }
        } else {
            showFilePicker(filePathCallback, fileChooserParams)
        }
        return true
    }

    private fun isMediaCaptureSupported(): Boolean {
        val permissions = arrayOf(Manifest.permission.CAMERA)
        return hasPermissions(activity, permissions) ||
                !hasDefinedPermission(activity, Manifest.permission.CAMERA)
    }

    private fun hasDefinedPermission(activity: Activity, permission: String): Boolean {
        return try {
            val info = activity.packageManager.getPackageInfo(activity.packageName, android.content.pm.PackageManager.GET_PERMISSIONS)
            info.requestedPermissions?.contains(permission) ?: false
        } catch (e: Exception) {
            false
        }
    }

    private fun showMediaCaptureOrFilePicker(
        filePathCallback: ValueCallback<Array<Uri?>?>,
        fileChooserParams: FileChooserParams,
        isVideo: Boolean
    ) {
        val shown = if (isVideo) {
            showVideoCapturePicker(filePathCallback)
        } else {
            showImageCapturePicker(filePathCallback)
        }
        if (!shown) {
            showFilePicker(filePathCallback, fileChooserParams)
        }
    }

    private fun showImageCapturePicker(filePathCallback: ValueCallback<Array<Uri?>?>): Boolean {
        val takePictureIntent = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
        if (takePictureIntent.resolveActivity(activity.packageManager) == null) {
            return false
        }
        val imageFileUri: Uri = try {
            createImageFileUri()
        } catch (ex: Exception) {
            return false
        }
        takePictureIntent.putExtra(MediaStore.EXTRA_OUTPUT, imageFileUri)
        activityListener = object : ActivityResultListener {
            override fun onActivityResult(result: ActivityResult?) {
                var res: Array<Uri?>? = null
                if (result?.resultCode == Activity.RESULT_OK) {
                    res = arrayOf(imageFileUri)
                }
                filePathCallback.onReceiveValue(res)
            }
        }
        activityLauncher.launch(takePictureIntent)
        return true
    }

    private fun showVideoCapturePicker(filePathCallback: ValueCallback<Array<Uri?>?>): Boolean {
        val takeVideoIntent = Intent(MediaStore.ACTION_VIDEO_CAPTURE)
        if (takeVideoIntent.resolveActivity(activity.packageManager) == null) {
            return false
        }
        activityListener = object : ActivityResultListener {
            override fun onActivityResult(result: ActivityResult?) {
                var res: Array<Uri?>? = null
                if (result?.resultCode == Activity.RESULT_OK) {
                    res = arrayOf(result.data!!.data)
                }
                filePathCallback.onReceiveValue(res)
            }
        }
        activityLauncher.launch(takeVideoIntent)
        return true
    }

    private fun showFilePicker(
        filePathCallback: ValueCallback<Array<Uri?>?>,
        fileChooserParams: FileChooserParams
    ) {
        val intent = fileChooserParams.createIntent()
        if (fileChooserParams.mode == FileChooserParams.MODE_OPEN_MULTIPLE) {
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        }
        if (fileChooserParams.acceptTypes.size > 1 || intent.type!!.startsWith(".")) {
            val validTypes = getValidTypes(fileChooserParams.acceptTypes)
            intent.putExtra(Intent.EXTRA_MIME_TYPES, validTypes)
            if (intent.type!!.startsWith(".")) {
                intent.type = validTypes[0]
            }
        }
        try {
            activityListener = object : ActivityResultListener {
                override fun onActivityResult(result: ActivityResult?) {
                    val res: Array<Uri?>?
                    val resultIntent = result?.data
                    if (result?.resultCode == Activity.RESULT_OK && resultIntent!!.clipData != null) {
                        val numFiles = resultIntent.clipData!!.itemCount
                        res = arrayOfNulls(numFiles)
                        for (i in 0 until numFiles) {
                            res[i] = resultIntent.clipData!!.getItemAt(i).uri
                        }
                    } else {
                        res = FileChooserParams.parseResult(
                            result?.resultCode ?: 0,
                            resultIntent
                        )
                    }
                    filePathCallback.onReceiveValue(res)
                }
            }
            activityLauncher.launch(intent)
        } catch (e: ActivityNotFoundException) {
            filePathCallback.onReceiveValue(null)
        }
    }

    private fun getValidTypes(currentTypes: Array<String>): Array<String> {
        val validTypes: MutableList<String> = ArrayList()
        val mtm = MimeTypeMap.getSingleton()
        for (mime in currentTypes) {
            if (mime.startsWith(".")) {
                val extension = mime.substring(1)
                val extensionMime = mtm.getMimeTypeFromExtension(extension)
                if (extensionMime != null && !validTypes.contains(extensionMime)) {
                    validTypes.add(extensionMime)
                }
            } else if (!validTypes.contains(mime)) {
                validTypes.add(mime)
            }
        }
        val validObj: Array<Any> = validTypes.toTypedArray()
        return Arrays.copyOf(validObj, validObj.size, Array<String>::class.java)
    }

    @Throws(IOException::class)
    private fun createImageFileUri(): Uri {
        val timeStamp = SimpleDateFormat("yyyyMMdd_HHmmss").format(Date())
        val imageFileName = "JPEG_${timeStamp}_"
        val storageDir = activity.getExternalFilesDir(Environment.DIRECTORY_PICTURES)
        val photoFile = File.createTempFile(imageFileName, ".jpg", storageDir)
        return FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", photoFile)
    }

    // ========== 控制台日志 ==========
    override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean {
        val msg = String.format(
            "File: %s - Line %d - Msg: %s",
            consoleMessage.sourceId(),
            consoleMessage.lineNumber(),
            consoleMessage.message()
        )
        android.util.Log.d("WebView", msg)
        return true
    }

    // ========== 标题 ==========
    override fun onReceivedTitle(view: WebView, title: String) {
        super.onReceivedTitle(view, title)
    }
}

package com.pwa.container

import android.util.Log
import fi.iki.elonen.NanoHTTPD
import java.io.ByteArrayInputStream
import java.io.File
import java.io.FileInputStream

/**
 * 本地 HTTP 代理服务器
 */
class ProxyServer(port: Int = 19315) : NanoHTTPD("localhost", port) {

    companion object {
        const val PORT = 19316
        const val TAG = "ProxyServer"
    }

    override fun serve(session: IHTTPSession): Response {
        val uri = session.uri
        Log.d(TAG, "Request: $uri")

        return when {
            uri.startsWith("/local/file/") -> handleLocalFile(session)
            else -> newFixedLengthResponse("OK")
        }
    }

    private fun handleLocalFile(session: IHTTPSession): Response {
        return try {
            val encodedPath = session.uri.substringAfter("/local/file/")
            if (encodedPath.isEmpty()) {
                return newFixedLengthResponse(Response.Status.BAD_REQUEST, "text/plain", "Missing file path")
            }

            val filePath = java.net.URLDecoder.decode(encodedPath, "UTF-8")
            Log.d(TAG, "Local file request: $filePath")

            val file = File(filePath)
            if (!file.exists() || !file.isFile) {
                return newFixedLengthResponse(Response.Status.NOT_FOUND, "text/plain", "File not found")
            }

            val mimeType = getMimeType(filePath)
            val rangeHeader = session.headers["range"]

            if (rangeHeader != null && rangeHeader.startsWith("bytes=")) {
                val rangeSpec = rangeHeader.substring(6)
                val parts = rangeSpec.split("-")
                val fileLength = file.length()

                val start = parts[0].toLongOrNull() ?: 0
                val end = if (parts.size > 1 && parts[1].isNotEmpty()) {
                    parts[1].toLongOrNull() ?: (fileLength - 1)
                } else {
                    fileLength - 1
                }.coerceAtMost(fileLength - 1)

                if (start > end || start >= fileLength) {
                    val errRes = newFixedLengthResponse(Response.Status.BAD_REQUEST, "text/plain", "Invalid range")
                    errRes.addHeader("Content-Range", "bytes */$fileLength")
                    return errRes
                }

                val length = end - start + 1
                val fis = FileInputStream(file)
                fis.skip(start)
                val buffer = ByteArray(length.toInt())
                fis.read(buffer)
                fis.close()

                val response = newFixedLengthResponse(
                    Response.Status.OK,
                    mimeType,
                    ByteArrayInputStream(buffer),
                    length
                )

                response.addHeader("Content-Range", "bytes $start-$end/$fileLength")
                response.addHeader("Accept-Ranges", "bytes")
                response.addHeader("Content-Length", length.toString())
                response.addHeader("Access-Control-Allow-Origin", "*")

                response
            } else {
                val fis = FileInputStream(file)
                val response = newFixedLengthResponse(
                    Response.Status.OK,
                    mimeType,
                    fis,
                    file.length()
                )

                response.addHeader("Accept-Ranges", "bytes")
                response.addHeader("Content-Length", file.length().toString())
                response.addHeader("Access-Control-Allow-Origin", "*")

                response
            }
        } catch (e: Exception) {
            Log.e(TAG, "Local file error: ${e.message}", e)
            newFixedLengthResponse(Response.Status.INTERNAL_ERROR, "application/json",
                "{\"error\": \"${e.message}\"}")
        }
    }

    private fun getMimeType(filePath: String): String {
        val ext = filePath.substringAfterLast(".", "").lowercase()
        return when (ext) {
            "mp3" -> "audio/mpeg"
            "flac" -> "audio/flac"
            "wav" -> "audio/wav"
            "ogg" -> "audio/ogg"
            "m4a" -> "audio/mp4"
            "aac" -> "audio/aac"
            "wma" -> "audio/x-ms-wma"
            "mp4" -> "video/mp4"
            "webm" -> "video/webm"
            "mkv" -> "video/x-matroska"
            "mov" -> "video/quicktime"
            "avi" -> "video/x-msvideo"
            "jpg", "jpeg" -> "image/jpeg"
            "png" -> "image/png"
            "gif" -> "image/gif"
            "webp" -> "image/webp"
            else -> "application/octet-stream"
        }
    }
}
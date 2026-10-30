package dev.slimmusic.app

import android.Manifest
import android.app.Activity
import android.content.ComponentName
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.provider.Settings
import androidx.core.content.FileProvider
import java.net.HttpURLConnection
import java.net.URL
import android.util.Base64
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import java.io.File

@InvokeArg
class HandlerArgs {
  lateinit var handler: Channel
}

@TauriPlugin(
  permissions = [
    Permission(strings = [Manifest.permission.READ_MEDIA_AUDIO], alias = "audio"),
    Permission(strings = [Manifest.permission.READ_EXTERNAL_STORAGE], alias = "storage")
  ]
)
class SlimNativePlugin(private val activity: Activity) : Plugin(activity) {
  private val main = Handler(Looper.getMainLooper())
  private var controllerFuture: ListenableFuture<MediaController>? = null
  private var controller: MediaController? = null
  private val pending = ArrayList<(MediaController) -> Unit>()
  private var channel: Channel? = null

  private val ticker = object : Runnable {
    override fun run() {
      val c = controller ?: return
      emitState(c)
      if (c.isPlaying) main.postDelayed(this, 500)
    }
  }

  private val listener = object : Player.Listener {
    override fun onIsPlayingChanged(isPlaying: Boolean) {
      controller?.let { emitState(it) }
      main.removeCallbacks(ticker)
      if (isPlaying) main.postDelayed(ticker, 500)
    }

    override fun onPlaybackStateChanged(state: Int) {
      val c = controller ?: return
      emitState(c)
      if (state == Player.STATE_ENDED) emit(JSObject().put("type", "ended"))
    }

    override fun onPlayerError(error: PlaybackException) {
      emit(JSObject().put("type", "error").put("message", error.errorCodeName + ": " + (error.message ?: "")))
    }
  }

  init {
    PlaybackBus.onRemote = { action ->
      main.post { emit(JSObject().put("type", "remote").put("action", action)) }
    }
  }

  override fun load(webView: WebView) {
    // 后台时也保持渲染进程优先级，避免队列逻辑（JS）被系统回收
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      webView.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, false)
    }
    // 返回键交给前端（关闭播放页等）；前端未接管时退到后台而不是销毁，播放继续
    (activity as? ComponentActivity)?.onBackPressedDispatcher?.addCallback(
      object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
          if (channel != null) emit(JSObject().put("type", "back"))
          else activity.moveTaskToBack(true)
        }
      }
    )
  }

  @Command
  fun moveToBack(invoke: Invoke) {
    main.post { activity.moveTaskToBack(true) }
    invoke.resolve()
  }

  private fun emit(o: JSObject) {
    try {
      channel?.send(o)
    } catch (_: Exception) {
    }
  }

  private fun stateObj(c: MediaController): JSObject {
    val dur = c.duration
    return JSObject()
      .put("type", "state")
      .put("playing", c.isPlaying)
      .put("playWhenReady", c.playWhenReady)
      .put("buffering", c.playbackState == Player.STATE_BUFFERING)
      .put("position", c.currentPosition.toDouble() / 1000.0)
      .put("duration", if (dur > 0) dur.toDouble() / 1000.0 else 0.0)
  }

  private fun emitState(c: MediaController) = emit(stateObj(c))

  /** 在主线程拿到已连接的 MediaController 后执行 */
  private fun withController(block: (MediaController) -> Unit) {
    main.post {
      val c = controller
      if (c != null) {
        block(c)
        return@post
      }
      pending.add(block)
      if (controllerFuture == null) {
        val token = SessionToken(activity, ComponentName(activity, PlaybackService::class.java))
        val f = MediaController.Builder(activity, token).buildAsync()
        controllerFuture = f
        f.addListener({
          try {
            val mc = f.get()
            mc.addListener(listener)
            controller = mc
            val q = ArrayList(pending)
            pending.clear()
            q.forEach { it(mc) }
          } catch (e: Exception) {
            controllerFuture = null
            pending.clear()
            emit(JSObject().put("type", "error").put("message", "controller: " + e.message))
          }
        }, MoreExecutors.directExecutor())
      }
    }
  }

  @Command
  fun setHandler(invoke: Invoke) {
    val args = invoke.parseArgs(HandlerArgs::class.java)
    channel = args.handler
    invoke.resolve()
  }

  @Command
  fun load(invoke: Invoke) {
    val a = invoke.getArgs()
    val url = a.getString("url", null)
    if (url.isNullOrEmpty()) {
      invoke.reject("missing url")
      return
    }
    val uri = if (url.startsWith("/")) Uri.fromFile(File(url)) else Uri.parse(url)
    val meta = MediaMetadata.Builder()
      .setTitle(a.getString("title", null))
      .setArtist(a.getString("artist", null))
      .setAlbumTitle(a.getString("album", null))
    a.getString("artwork", null)?.takeIf { it.isNotEmpty() }?.let { meta.setArtworkUri(Uri.parse(it)) }
    val item = MediaItem.Builder()
      .setUri(uri)
      .setMediaId(a.getString("id", url) ?: url)
      .setMediaMetadata(meta.build())
      .build()
    val posMs = (a.optDouble("position", 0.0) * 1000).toLong()
    val autoplay = a.getBoolean("autoplay", true) ?: true
    withController { c ->
      c.setMediaItem(item, posMs)
      c.prepare()
      c.playWhenReady = autoplay
      invoke.resolve()
    }
  }

  @Command
  fun play(invoke: Invoke) = withController { c ->
    if (c.playbackState == Player.STATE_ENDED) c.seekTo(0)
    if (c.playbackState == Player.STATE_IDLE) c.prepare()
    c.play(); invoke.resolve()
  }

  @Command
  fun pause(invoke: Invoke) = withController { c -> c.pause(); invoke.resolve() }

  @Command
  fun stop(invoke: Invoke) = withController { c -> c.stop(); c.clearMediaItems(); invoke.resolve() }

  @Command
  fun seek(invoke: Invoke) {
    val ms = (invoke.getArgs().optDouble("position", 0.0) * 1000).toLong()
    withController { c -> c.seekTo(ms); emitState(c); invoke.resolve() }
  }

  @Command
  fun setVolume(invoke: Invoke) {
    val v = invoke.getArgs().optDouble("volume", 1.0).toFloat().coerceIn(0f, 1f)
    withController { c -> c.volume = v; invoke.resolve() }
  }

  @Command
  fun getState(invoke: Invoke) = withController { c -> invoke.resolve(stateObj(c)) }

  // ---------- 本地音乐（MediaStore） ----------

  private fun audioAlias(): String =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) "audio" else "storage"

  @Command
  fun requestAudioPermission(invoke: Invoke) {
    if (getPermissionState(audioAlias()) == PermissionState.GRANTED) {
      invoke.resolve(JSObject().put("granted", true))
    } else {
      requestPermissionForAlias(audioAlias(), invoke, "audioPermissionCallback")
    }
  }

  @PermissionCallback
  fun audioPermissionCallback(invoke: Invoke) {
    invoke.resolve(JSObject().put("granted", getPermissionState(audioAlias()) == PermissionState.GRANTED))
  }

  @Command
  fun queryAudio(invoke: Invoke) {
    if (getPermissionState(audioAlias()) != PermissionState.GRANTED) {
      invoke.reject("permission denied", "PERMISSION")
      return
    }
    Thread {
      try {
        val out = JSArray()
        val cols = arrayOf(
          MediaStore.Audio.Media._ID,
          MediaStore.Audio.Media.TITLE,
          MediaStore.Audio.Media.ARTIST,
          MediaStore.Audio.Media.ALBUM,
          MediaStore.Audio.Media.ALBUM_ID,
          MediaStore.Audio.Media.DURATION,
          MediaStore.Audio.Media.DATA,
          MediaStore.Audio.Media.DISPLAY_NAME
        )
        val base = MediaStore.Audio.Media.EXTERNAL_CONTENT_URI
        activity.contentResolver.query(
          base, cols,
          "${MediaStore.Audio.Media.IS_MUSIC} != 0", null,
          "${MediaStore.Audio.Media.TITLE} COLLATE NOCASE ASC"
        )?.use { cur ->
          val iId = cur.getColumnIndexOrThrow(cols[0])
          val iTitle = cur.getColumnIndexOrThrow(cols[1])
          val iArtist = cur.getColumnIndexOrThrow(cols[2])
          val iAlbum = cur.getColumnIndexOrThrow(cols[3])
          val iAlbumId = cur.getColumnIndexOrThrow(cols[4])
          val iDur = cur.getColumnIndexOrThrow(cols[5])
          val iData = cur.getColumnIndexOrThrow(cols[6])
          val iName = cur.getColumnIndexOrThrow(cols[7])
          while (cur.moveToNext()) {
            val id = cur.getLong(iId)
            val artist = cur.getString(iArtist)
            val o = JSObject()
              .put("uri", Uri.withAppendedPath(base, id.toString()).toString())
              .put("title", cur.getString(iTitle) ?: cur.getString(iName) ?: "")
              .put("artist", if (artist == null || artist == "<unknown>") "" else artist)
              .put("album", cur.getString(iAlbum) ?: "")
              .put("durationMs", cur.getLong(iDur))
              .put("path", cur.getString(iData) ?: "")
              .put("albumArt", "content://media/external/audio/albumart/" + cur.getLong(iAlbumId))
            out.put(o)
          }
        }
        invoke.resolve(JSObject().put("tracks", out))
      } catch (e: Exception) {
        invoke.reject(e.message ?: "query failed")
      }
    }.start()
  }

  /** 尝试读取音频旁同名 .lrc（受分区存储限制，读不到就返回空）。 */
  @Command
  fun readLrc(invoke: Invoke) {
    val uri = invoke.getArgs().getString("uri", null) ?: ""
    Thread {
      var text = ""
      try {
        var path: String? = null
        activity.contentResolver.query(Uri.parse(uri), arrayOf(MediaStore.Audio.Media.DATA), null, null, null)?.use {
          if (it.moveToFirst()) path = it.getString(0)
        }
        path?.let { p ->
          val f = File(p)
          val lrc = File(f.parentFile, f.nameWithoutExtension + ".lrc")
          if (lrc.canRead()) text = lrc.readText()
          else {
            // MediaStore 中按文件名查找（部分机型将 .lrc 索引为 Files）
            val sel = "${MediaStore.MediaColumns.DATA} = ?"
            activity.contentResolver.query(
              MediaStore.Files.getContentUri("external"), arrayOf(MediaStore.MediaColumns._ID),
              sel, arrayOf(lrc.absolutePath), null
            )?.use { c ->
              if (c.moveToFirst()) {
                val u = Uri.withAppendedPath(MediaStore.Files.getContentUri("external"), c.getLong(0).toString())
                activity.contentResolver.openInputStream(u)?.use { s -> text = s.readBytes().toString(Charsets.UTF_8) }
              }
            }
          }
        }
      } catch (_: Exception) {
      }
      invoke.resolve(JSObject().put("lrc", text))
    }.start()
  }

  // ---------- 杂项 ----------

  @Command
  fun openUrl(invoke: Invoke) {
    val url = invoke.getArgs().getString("url", null) ?: ""
    try {
      val i = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(i)
      invoke.resolve(JSObject().put("ok", true))
    } catch (e: Exception) {
      invoke.resolve(JSObject().put("ok", false))
    }
  }

  /** 保存 PNG（base64）到相册 Pictures/SlimMusic */
  @Command
  fun saveImage(invoke: Invoke) {
    val a = invoke.getArgs()
    val b64 = a.getString("data", null) ?: ""
    val name = a.getString("name", null) ?: "slimmusic-${System.currentTimeMillis()}.png"
    try {
      val bytes = Base64.decode(b64.substringAfter(","), Base64.DEFAULT)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        val v = ContentValues().apply {
          put(MediaStore.Images.Media.DISPLAY_NAME, name)
          put(MediaStore.Images.Media.MIME_TYPE, "image/png")
          put(MediaStore.Images.Media.RELATIVE_PATH, Environment.DIRECTORY_PICTURES + "/SlimMusic")
        }
        val u = activity.contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, v)
          ?: throw Exception("insert failed")
        activity.contentResolver.openOutputStream(u)?.use { it.write(bytes) }
      } else {
        @Suppress("DEPRECATION")
        val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES), "SlimMusic")
        dir.mkdirs()
        File(dir, name).writeBytes(bytes)
      }
      invoke.resolve(JSObject().put("ok", true))
    } catch (e: Exception) {
      invoke.reject(e.message ?: "save failed")
    }
  }

  // ---------- 应用内更新（APK） ----------

  @Volatile private var dlState = "idle"
  @Volatile private var dlReceived = 0L
  @Volatile private var dlTotal = 0L
  @Volatile private var dlError = ""
  private var pendingInstall: File? = null

  private fun apkFile() = File(File(activity.cacheDir, "update").apply { mkdirs() }, "update.apk")

  @Command
  fun downloadApk(invoke: Invoke) {
    val url = invoke.getArgs().getString("url", null)
    if (url.isNullOrEmpty()) { invoke.reject("missing url"); return }
    if (dlState == "downloading") { invoke.resolve(); return }
    dlState = "downloading"; dlReceived = 0; dlTotal = 0; dlError = ""
    Thread {
      try {
        var u = URL(url)
        var conn: HttpURLConnection
        var hops = 0
        while (true) {
          conn = u.openConnection() as HttpURLConnection
          conn.instanceFollowRedirects = false
          conn.connectTimeout = 15000
          conn.readTimeout = 30000
          val code = conn.responseCode
          if (code in 300..399 && hops++ < 8) {
            u = URL(u, conn.getHeaderField("Location")); conn.disconnect(); continue
          }
          if (code != 200) throw Exception("HTTP $code")
          break
        }
        dlTotal = conn.contentLengthLong
        val f = apkFile()
        conn.inputStream.use { inp ->
          f.outputStream().use { out ->
            val buf = ByteArray(64 * 1024)
            while (true) {
              val n = inp.read(buf)
              if (n < 0) break
              out.write(buf, 0, n)
              dlReceived += n
            }
          }
        }
        conn.disconnect()
        dlState = "done"
      } catch (e: Exception) {
        dlError = e.message ?: "download failed"
        dlState = "error"
      }
    }.start()
    invoke.resolve()
  }

  @Command
  fun downloadStatus(invoke: Invoke) {
    invoke.resolve(
      JSObject().put("state", dlState).put("received", dlReceived.toDouble())
        .put("total", dlTotal.toDouble()).put("error", dlError)
    )
  }

  private fun canInstall() =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.O || activity.packageManager.canRequestPackageInstalls()

  private fun launchInstaller(f: File) {
    val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", f)
    val i = Intent(Intent.ACTION_VIEW)
      .setDataAndType(uri, "application/vnd.android.package-archive")
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    activity.startActivity(i)
  }

  @Command
  fun installApk(invoke: Invoke) {
    val f = apkFile()
    if (!f.exists()) { invoke.reject("apk missing"); return }
    main.post {
      try {
        if (canInstall()) {
          launchInstaller(f)
          invoke.resolve(JSObject().put("needPermission", false))
        } else {
          pendingInstall = f
          val i = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.packageName))
          activity.startActivity(i)
          invoke.resolve(JSObject().put("needPermission", true))
        }
      } catch (e: Exception) {
        invoke.reject(e.message ?: "install failed")
      }
    }
  }

  override fun onResume(activity: androidx.appcompat.app.AppCompatActivity) {
    val f = pendingInstall ?: return
    if (canInstall()) {
      pendingInstall = null
      main.post { try { launchInstaller(f) } catch (_: Exception) {} }
    }
  }

  override fun onDestroy() {
    main.removeCallbacks(ticker)
    controller?.removeListener(listener)
    controllerFuture?.let { MediaController.releaseFuture(it) }
    controller = null
    controllerFuture = null
    PlaybackBus.onRemote = null
  }
}

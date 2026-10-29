package dev.slimmusic.app

import android.app.PendingIntent
import android.content.Intent
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/** 通知栏 / 锁屏 / 耳机的「上一首 / 下一首」交给前端队列处理。 */
object PlaybackBus {
  @Volatile var onRemote: ((String) -> Unit)? = null
}

/**
 * 前台媒体播放服务：ExoPlayer + MediaSession。
 * 只承载「当前一首」，队列逻辑在前端；Media3 自动维护前台通知与锁屏控制。
 */
@OptIn(UnstableApi::class)
class PlaybackService : MediaSessionService() {
  private var session: MediaSession? = null

  override fun onCreate() {
    super.onCreate()
    val exo = ExoPlayer.Builder(this)
      .setAudioAttributes(
        AudioAttributes.Builder()
          .setUsage(C.USAGE_MEDIA)
          .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
          .build(),
        true
      )
      .setHandleAudioBecomingNoisy(true)
      .setWakeMode(C.WAKE_MODE_NETWORK)
      .build()

    val player = object : ForwardingPlayer(exo) {
      private val extra = listOf(
        Player.COMMAND_SEEK_TO_NEXT,
        Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
        Player.COMMAND_SEEK_TO_PREVIOUS,
        Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM
      )

      override fun getAvailableCommands(): Player.Commands {
        val b = super.getAvailableCommands().buildUpon()
        extra.forEach { b.add(it) }
        return b.build()
      }

      override fun isCommandAvailable(command: Int): Boolean =
        command in extra || super.isCommandAvailable(command)

      override fun hasNextMediaItem(): Boolean = true
      override fun hasPreviousMediaItem(): Boolean = true
      override fun seekToNext() = remote("next")
      override fun seekToNextMediaItem() = remote("next")
      override fun seekToPrevious() = remote("prev")
      override fun seekToPreviousMediaItem() = remote("prev")

      private fun remote(action: String) {
        PlaybackBus.onRemote?.invoke(action)
      }
    }

    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    session = MediaSession.Builder(this, player).setSessionActivity(open).build()
  }

  override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

  override fun onTaskRemoved(rootIntent: Intent?) {
    val p = session?.player
    if (p == null || !p.playWhenReady || p.mediaItemCount == 0) {
      stopSelf()
    }
  }

  override fun onDestroy() {
    session?.run {
      player.release()
      release()
    }
    session = null
    super.onDestroy()
  }
}

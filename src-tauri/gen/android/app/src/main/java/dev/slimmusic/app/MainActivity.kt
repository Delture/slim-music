package dev.slimmusic.app

import android.os.Bundle

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // 不使用 edge-to-edge：旧版 WebView 不提供系统栏 safe-area，内容会被状态栏遮挡
    super.onCreate(savedInstanceState)
  }
}

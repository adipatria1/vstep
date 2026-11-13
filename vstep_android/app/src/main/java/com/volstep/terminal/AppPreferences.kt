package com.volstep.terminal

import android.content.Context
import android.content.SharedPreferences

class AppPreferences(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    var serverUrl: String
        get() {
            val url = prefs.getString(KEY_SERVER_URL, DEFAULT_URL) ?: DEFAULT_URL
            return if (url.endsWith("/")) url.dropLast(1) else url
        }
        set(value) {
            val cleanUrl = value.trim().let { if (it.endsWith("/")) it.dropLast(1) else it }
            prefs.edit().putString(KEY_SERVER_URL, cleanUrl).apply()
        }

    var isNotificationEnabled: Boolean
        get() = prefs.getBoolean(KEY_NOTIF_ENABLED, true)
        set(value) = prefs.edit().putBoolean(KEY_NOTIF_ENABLED, value).apply()

    companion object {
        private const val PREFS_NAME = "volstep_terminal_prefs"
        private const val KEY_SERVER_URL = "key_server_url"
        private const val KEY_NOTIF_ENABLED = "key_notif_enabled"

        // Default URL: fallback port 5050
        const val DEFAULT_URL = "http://10.0.2.2:5050" // 10.0.2.2 adalah localhost host machine di emulator Android
    }
}

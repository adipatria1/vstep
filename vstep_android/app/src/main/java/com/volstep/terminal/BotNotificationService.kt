package com.volstep.terminal

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit

class BotNotificationService : Service() {

    private val serviceScope = CoroutineScope(Dispatchers.IO + Job())
    private lateinit var prefs: AppPreferences
    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(5, TimeUnit.SECONDS)
        .build()

    private var lastSeenLogCount = 0
    private var lastNotificationTime = 0L

    override fun onCreate() {
        super.onCreate()
        prefs = AppPreferences(this)
        createNotificationChannels()
        startForeground(FOREGROUND_NOTIF_ID, createForegroundNotification("VolStep Bot Monitor Siap"))
        startPollingLogs()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannels() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

            // Channel 1: Sticky Foreground Service
            val statusChannel = NotificationChannel(
                CHANNEL_STATUS_ID,
                "Status Bot Aktif",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Menjaga koneksi monitoring bot tetap hidup di background"
            }

            // Channel 2: High Priority Trade Signals & Orders
            val tradeChannel = NotificationChannel(
                CHANNEL_TRADE_ID,
                "Sinyal Trading & Eksekusi Order",
                NotificationManager.IMPORTANCE_HIGH
            ).apply {
                description = "Notifikasi saat order Pre-Close Sniper atau TP/SL tercapai"
                enableVibration(true)
            }

            manager.createNotificationChannel(statusChannel)
            manager.createNotificationChannel(tradeChannel)
        }
    }

    private fun createForegroundNotification(statusText: String): Notification {
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        return NotificationCompat.Builder(this, CHANNEL_STATUS_ID)
            .setContentTitle("VolStep Futures Bot")
            .setContentText(statusText)
            .setSmallIcon(R.drawable.ic_bot)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()
    }

    private fun showTradeNotification(title: String, message: String) {
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 1, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val notif = NotificationCompat.Builder(this, CHANNEL_TRADE_ID)
            .setContentTitle(title)
            .setContentText(message)
            .setStyle(NotificationCompat.BigTextStyle().bigText(message))
            .setSmallIcon(R.drawable.ic_app_logo)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()

        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(System.currentTimeMillis().toInt(), notif)
    }

    private fun startPollingLogs() {
        serviceScope.launch {
            while (isActive) {
                delay(3000)
                if (!prefs.isNotificationEnabled) continue

                val serverUrl = prefs.serverUrl
                try {
                    val req = Request.Builder()
                        .url("$serverUrl/api/demo/status")
                        .build()

                    httpClient.newCall(req).execute().use { resp ->
                        if (resp.isSuccessful) {
                            val body = resp.body?.string() ?: return@use
                            val json = JSONObject(body)
                            val isRunning = json.optBoolean("is_running", false)
                            val logs = json.optJSONArray("logs")

                            // Update text status notification
                            val statusMsg = if (isRunning) "Bot Running | WebSocket Sniper Aktif" else "Bot Standby"
                            val notifManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                            notifManager.notify(FOREGROUND_NOTIF_ID, createForegroundNotification(statusMsg))

                            // Periksa log terbaru untuk sinyal / eksekusi order
                            if (logs != null && logs.length() > lastSeenLogCount) {
                                for (i in lastSeenLogCount until logs.length()) {
                                    val entry = logs.getJSONObject(i)
                                    val level = entry.optString("level")
                                    val msg = entry.optString("message")

                                    if (level == "trade" || msg.contains("BERHASIL") || msg.contains("SNIPER") || msg.contains("EXIT")) {
                                        showTradeNotification("⚡ VolStep Order Triggered", msg)
                                    }
                                }
                                lastSeenLogCount = logs.length()
                            }
                        }
                    }
                } catch (e: Exception) {
                    // Host mungkin offline atau sedang transisi
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
    }

    companion object {
        const val FOREGROUND_NOTIF_ID = 1001
        const val CHANNEL_STATUS_ID = "channel_bot_status"
        const val CHANNEL_TRADE_ID = "channel_trade_signals"
    }
}

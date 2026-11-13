package com.volstep.terminal

import android.Manifest
import android.annotation.SuppressLint
import android.app.AlertDialog
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.widget.Toast
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.volstep.terminal.databinding.ActivityMainBinding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.util.concurrent.TimeUnit

class MainActivity : AppCompatActivity() {

    private lateinit var binding: ActivityMainBinding
    private lateinit var prefs: AppPreferences
    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(3, TimeUnit.SECONDS)
        .readTimeout(3, TimeUnit.SECONDS)
        .build()

    private var currentPath: String = "/"
    private var backPressedTime: Long = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        prefs = AppPreferences(this)

        requestAppPermissions()
        setupWebView()
        setupBottomNav()
        setupToolbarActions()
        setupBackHandler()
        startBotService()

        loadCurrentPage()
    }

    private fun requestAppPermissions() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED
            ) {
                ActivityCompat.requestPermissions(
                    this,
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    101
                )
            }
        }
    }

    private fun startBotService() {
        val serviceIntent = Intent(this, BotNotificationService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(serviceIntent)
        } else {
            startService(serviceIntent)
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun setupWebView() {
        with(binding.webView.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            useWideViewPort = true
            loadWithOverviewMode = true
            setSupportZoom(true)
            builtInZoomControls = true
            displayZoomControls = false
            cacheMode = WebSettings.LOAD_DEFAULT
            mixedContentMode = WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
        }

        binding.webView.webViewClient = object : WebViewClient() {
            override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                super.onPageStarted(view, url, favicon)
                binding.layoutErrorOverlay.visibility = View.GONE
                binding.statusDot.setBackgroundColor(ContextCompat.getColor(this@MainActivity, R.color.warn_yellow))
            }

            override fun onPageFinished(view: WebView?, url: String?) {
                super.onPageFinished(view, url)
                binding.swipeRefresh.isRefreshing = false
                binding.statusDot.setBackgroundColor(ContextCompat.getColor(this@MainActivity, R.color.good_green))
            }

            override fun onReceivedError(view: WebView?, request: WebResourceRequest?, error: WebResourceError?) {
                super.onReceivedError(view, request, error)
                if (request?.isForMainFrame == true) {
                    binding.swipeRefresh.isRefreshing = false
                    binding.statusDot.setBackgroundColor(ContextCompat.getColor(this@MainActivity, R.color.bad_red))
                    showConnectionErrorOverlay()
                }
            }

            override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                val url = request?.url?.toString() ?: return false
                if (url.startsWith("http://") || url.startsWith("https://")) {
                    return false
                }
                // External link handler
                try {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
                    startActivity(intent)
                } catch (e: Exception) {
                    // Ignore
                }
                return true
            }
        }

        binding.webView.webChromeClient = object : WebChromeClient() {
            override fun onConsoleMessage(consoleMessage: ConsoleMessage?): Boolean {
                return super.onConsoleMessage(consoleMessage)
            }
        }

        binding.swipeRefresh.setOnRefreshListener {
            binding.webView.reload()
        }
    }

    private fun setupBottomNav() {
        binding.bottomNav.setOnItemSelectedListener { item ->
            when (item.itemId) {
                R.id.menu_home -> {
                    currentPath = "/"
                    loadCurrentPage()
                    true
                }
                R.id.menu_backtest -> {
                    currentPath = "/backtest"
                    loadCurrentPage()
                    true
                }
                R.id.menu_demo -> {
                    currentPath = "/demo"
                    loadCurrentPage()
                    true
                }
                R.id.menu_settings -> {
                    currentPath = "/settings"
                    loadCurrentPage()
                    true
                }
                else -> false
            }
        }
    }

    private fun setupToolbarActions() {
        binding.btnServerConfig.setOnClickListener {
            showServerConfigDialog()
        }

        binding.btnRetryConn.setOnClickListener {
            binding.layoutErrorOverlay.visibility = View.GONE
            loadCurrentPage()
        }

        binding.btnChangeIpError.setOnClickListener {
            showServerConfigDialog()
        }
    }

    private fun setupBackHandler() {
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (binding.webView.canGoBack()) {
                    binding.webView.goBack()
                } else {
                    if (System.currentTimeMillis() - backPressedTime < 2000) {
                        finish()
                    } else {
                        backPressedTime = System.currentTimeMillis()
                        Toast.makeText(this@MainActivity, "Tekan sekali lagi untuk keluar", Toast.LENGTH_SHORT).show()
                    }
                }
            }
        })
    }

    private fun loadCurrentPage() {
        val fullUrl = prefs.serverUrl + currentPath
        binding.tvCurrentTargetUrl.text = "Target: $fullUrl"
        binding.webView.loadUrl(fullUrl)
    }

    private fun showConnectionErrorOverlay() {
        binding.layoutErrorOverlay.visibility = View.VISIBLE
    }

    private fun showServerConfigDialog() {
        val dialogView = LayoutInflater.from(this).inflate(R.layout.dialog_server_config, null)
        val etUrl = dialogView.findViewById<EditText>(R.id.et_server_url)
        val tvResult = dialogView.findViewById<TextView>(R.id.tv_test_result)
        val btnTest = dialogView.findViewById<Button>(R.id.btn_dialog_test)
        val btnCancel = dialogView.findViewById<Button>(R.id.btn_dialog_cancel)
        val btnSave = dialogView.findViewById<Button>(R.id.btn_dialog_save)

        etUrl.setText(prefs.serverUrl)

        val dialog = AlertDialog.Builder(this)
            .setView(dialogView)
            .setCancelable(true)
            .create()

        btnTest.setOnClickListener {
            val candidateUrl = etUrl.text.toString().trim()
            if (candidateUrl.isEmpty()) {
                tvResult.text = "URL tidak boleh kosong"
                tvResult.setTextColor(ContextCompat.getColor(this, R.color.bad_red))
                tvResult.visibility = View.VISIBLE
                return@setOnClickListener
            }

            tvResult.text = "Menguji koneksi ke $candidateUrl..."
            tvResult.setTextColor(ContextCompat.getColor(this, R.color.warn_yellow))
            tvResult.visibility = View.VISIBLE

            CoroutineScope(Dispatchers.IO).launch {
                try {
                    val req = Request.Builder().url("$candidateUrl/api/demo/status").build()
                    val resp = httpClient.newCall(req).execute()
                    withContext(Dispatchers.Main) {
                        if (resp.isSuccessful) {
                            tvResult.text = "✓ Terhubung! Server VolStep aktif."
                            tvResult.setTextColor(ContextCompat.getColor(this@MainActivity, R.color.good_green))
                        } else {
                            tvResult.text = "Respons server: ${resp.code}"
                            tvResult.setTextColor(ContextCompat.getColor(this@MainActivity, R.color.warn_yellow))
                        }
                    }
                } catch (e: Exception) {
                    withContext(Dispatchers.Main) {
                        tvResult.text = "Gagal terhubung: ${e.message}"
                        tvResult.setTextColor(ContextCompat.getColor(this@MainActivity, R.color.bad_red))
                    }
                }
            }
        }

        btnCancel.setOnClickListener {
            dialog.dismiss()
        }

        btnSave.setOnClickListener {
            val candidateUrl = etUrl.text.toString().trim()
            if (candidateUrl.isNotEmpty()) {
                prefs.serverUrl = candidateUrl
                dialog.dismiss()
                Toast.makeText(this, "Alamat server diperbarui!", Toast.LENGTH_SHORT).show()
                binding.layoutErrorOverlay.visibility = View.GONE
                loadCurrentPage()
            }
        }

        dialog.show()
    }
}

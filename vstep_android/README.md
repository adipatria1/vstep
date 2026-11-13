# 📱 VolStep Terminal - Versi Android (APK)

Aplikasi native Android untuk memonitor, mengontrol, dan menerima notifikasi real-time dari **VolStep Binance Futures Trading Bot**.

---

## 🏗️ 1. Konsep & Arsitektur Mobile Bot

Mengapa menggunakan arsitektur **Mobile Client Controller + Bot Engine Server (24/7)**?

1. **Anti Doze Mode / Battery Killer Android**:
   - Sistem operasi Android (terutama MIUI, OneUI, ColorOS) secara agresif membunuh proses background yang berjalan lama untuk menghemat baterai.
   - Jika bot berjalan murni di dalam HP, ketika layar mati atau sinyal seluler drop, koneksi WebSocket Binance bisa terputus dan eksekusi Pre-Close Sniper terlewat.
2. **Koneksi Stabil & Cepat ke Binance**:
   - Mesin bot Rust (`vstep_rust`) berjalan 24/7 di PC, Laptop, atau VPS (Linux/Cloud) dengan jaringan kabel/fiber berkecepatan tinggi.
3. **Kontrol Penuh dari Genggaman HP**:
   - Aplikasi Android ini berfungsi sebagai **Remote Mobile Dashboard**.
   - Dilengkapi **Android Foreground Service** & **Push Notification**: HP Anda akan bergetar/berbunyi seketika saat order Pre-Close Sniper atau TP/SL tercapai.

---

## 🌟 2. Fitur Utama Versi Android

- **Native Bottom Navigation Bar**:
  - 🏠 **Dashboard**: Ringkasan saldo, menu cepat, status terminal.
  - 📊 **Strategy**: Akses penuh ke Strategy Finder 105 strategi [vstep.txt](file:///home/tomang/Desktop/vstep/vstep.txt).
  - 🤖 **Demo Bot**: Kontrol live paper trading, monitor open positions, dan log real-time WebSocket.
  - ⚙️ **Settings**: Pengaturan kredensial API Key & Secret Key Binance.
- **Dynamic Server IP Switcher**:
  - Tombol ikon server di pojok kanan atas untuk mengganti alamat IP server bot secara instan.
  - Fitur **Tes Koneksi**: Menguji koneksi ke PC/VPS sebelum menyimpan.
- **Background Notification Service (`BotNotificationService`)**:
  - Sticky notification status bot di status bar Android.
  - High-priority alert saat sinyal terdeteksi dan order market berhasil dibuka/ditutup.
- **Pull to Refresh**: Tarik layar ke bawah untuk reload halaman secara instan.
- **Hardware Back Button Handling**: Tombol back HP menavigasi riwayat halaman web; konfirmasi ganda sebelum keluar aplikasi.

---

## 📂 3. Struktur Folder Project `vstep_android`

```
vstep_android/
├── app/
│   ├── build.gradle                 # Konfigurasi modul Android (SDK 34, MinSDK 26, Dependencies)
│   ├── proguard-rules.pro           # Proteksi & optimasi kode R8/ProGuard
│   └── src/
│       └── main/
│           ├── AndroidManifest.xml  # Izin Internet, Foreground Service, Notifikasi
│           ├── java/com/volstep/terminal/
│           │   ├── MainActivity.kt           # WebView Controller, Bottom Nav, Back Handler, IP Dialog
│           │   ├── BotNotificationService.kt # Foreground Service & Trigger Notifikasi Android
│           │   └── AppPreferences.kt         # Penyimpanan lokal SharedPreferences (Server URL)
│           ├── res/
│           │   ├── layout/
│           │   │   ├── activity_main.xml         # Desain layar HP (Toolbar, WebView, BottomNav, Error Overlay)
│           │   │   └── dialog_server_config.xml  # Modal ganti IP server & tes koneksi
│           │   ├── menu/bottom_nav_menu.xml      # Navigasi tab bawah
│           │   ├── values/                       # Warna dark theme, teks bahasa Indonesia, tema
│           │   ├── drawable/                     # Ikon vector (Logo, Dashboard, Chart, Bot, Settings, Server)
│           │   └── xml/network_security_config.xml # Izin koneksi HTTP LAN / WiFi lokal
│           └── assets/
│               └── offline_portal.html      # Tampilan portal bantuan offline
├── build.gradle                     # Top-level Gradle script
├── settings.gradle                  # Definisi nama project & repository
├── gradle.properties                # Konfigurasi memori JVM & AndroidX
├── gradlew                          # Gradle wrapper script untuk Linux/macOS
├── build_apk.sh                     # Helper script kompilasi APK otomatis
└── README.md                        # Panduan lengkap ini
```

---

## 🛠️ 4. Cara Build APK (Menghasilkan File `.apk`)

### Cara 1: Menggunakan Android Studio (Paling Mudah)
1. Buka aplikasi **Android Studio** di komputer Anda.
2. Pilih menu **File** $\rightarrow$ **Open...**
3. Arahkan ke folder:
   `/home/tomang/Desktop/vstep/vstep_android`
4. Tunggu beberapa detik hingga proses **Gradle Sync** selesai secara otomatis.
5. Klik menu atas:
   **Build** $\rightarrow$ **Build Bundle(s) / APK(s)** $\rightarrow$ **Build APK(s)**.
6. Android Studio akan mengompilasi project, dan setelah selesai akan muncul notifikasi:
   > *"APK(s) generated successfully."*
7. Klik **locate** pada notifikasi tersebut. File APK berada di:
   `app/build/outputs/apk/debug/app-debug.apk`

---

### Cara 2: Menggunakan Command Line (Terminal)
Jika di PC/Laptop Anda sudah terpasang Java JDK dan Android SDK:
```bash
cd /home/tomang/Desktop/vstep/vstep_android
./build_apk.sh
```
Atau langsung dengan Gradle:
```bash
./gradlew assembleDebug
```
File APK akan otomatis terbuat di: `app/build/outputs/apk/debug/app-debug.apk`.

---

## 📲 5. Cara Pasang di HP Android & Menghubungkan ke Bot

1. **Kirim File APK ke HP**:
   - Kirim file `app-debug.apk` ke HP Anda melalui kabel data USB, WhatsApp, Telegram, atau Google Drive.
   - Buka file APK di HP dan pilih **Install**.
2. **Cek Alamat IP PC/Laptop Anda**:
   - Buka terminal di PC Linux Anda, ketik:
     ```bash
     hostname -I
     ```
     Misal muncul IP: `192.168.1.15`.
3. **Pastikan Bot Rust Berjalan**:
   - Jalankan bot di PC:
     ```bash
     cd /home/tomang/Desktop/vstep/vstep_rust
     ./run.sh
     ```
4. **Hubungkan dari HP**:
   - Buka aplikasi **VolStep Terminal** di HP Android Anda.
   - Jika belum terhubung, klik ikon **Server (di kanan atas)**.
   - Masukkan alamat IP PC Anda: `http://192.168.1.15:5050` (sesuaikan dengan IP PC).
   - Klik **Tes Koneksi**. Jika muncul tanda hijau `✓ Terhubung!`, klik **Simpan & Hubungkan**.
5. Selesai! Halaman bot, backtest, dan trading demo sekarang tampil penuh di layar HP Anda dan siap digunakan kapan saja.

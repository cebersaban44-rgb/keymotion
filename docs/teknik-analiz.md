# AJAZZ AK980 PRO — Teknik Analiz

Tarih: 2026-11-10. Bu rapor iki kaynaktan oluşur:

- **[D] Donanımda doğrulandı.** Bu bilgisayarda klavyeye bağlanıp okundu.
- **[A] Araştırma.** Resmi kılavuz ve topluluk tersine mühendislik çalışmalarından derlendi. Kablo bağlandığında doğrulanması gerekir.

---

## 1. Bu bilgisayarda tespit edilenler [D]

| Alan | Değer |
|---|---|
| Bağlantı | **Bluetooth LE**. Ad: `AK980 PRO-1` (BT kanal 1). MAC: `E4:69:E5:xx:xx:xx` (cihaza özgü kısım gizlendi) |
| PnP ID (0x2A50) | `01 45 0C FE FE 38 01`: kaynak BT SIG, **VID 0x0C45 (SONiX)**, **PID 0xFEFE**, rev 0x0138 |
| BT modül yazılımı | `2024.07.26 SVN0138` |
| Pil (0x180F) | **%94** (`0x5E`), bildirim (notify) destekliyor |
| BLE bağlantı | interval 7.5–11.25 ms, latency 20, timeout 3 s. Bluetooth tarama hızı 125 Hz civarı. |
| Cihaz bilgisi (0x180A) | Üretici, model, seri ve donanım alanlarında SDK şablon metinleri var ("Manufacturer Name" vb.) |
| Windows sürücüsü | Standart Microsoft HID. **AJAZZ yazılımı kurulu değil.** |
| USB geçmişi | Klavye bu bilgisayara **hiç kabloyla ya da 2.4G alıcıyla bağlanmamış**. |

### GATT servisleri

| Servis | İçerik |
|---|---|
| 0x1800 | Genel Erişim (ad, tercih edilen bağlantı parametreleri) |
| 0x1801 | Genel Öznitelik (Service Changed) |
| 0x180A | Cihaz bilgisi (şablon metinler, SW rev, PnP ID) |
| 0x180F | Pil seviyesi (okuma ve bildirim) |
| 0x1813 | Tarama parametreleri |
| 0x1812 | HID. Windows tarafından kilitli; doğrudan erişilemez. |
| **0xFEE0 / 0xFEE1** | **"OTA Channel"**. WCH BLE çipinin yazılım güncelleme kanalı. **ASLA YAZILMAYACAK.** |

### Bluetooth HID raporları

| Rapor ID | İşlev |
|---|---|
| 0x01 | Sistem kontrolü (Power / Sleep / Wake) |
| 0x02 | Boot klavye: 6 tuş ve modifier, Apple Fn baytı, **LED çıkışı** (Num/Caps/Scroll) |
| 0x03 | Fare: 5 tuş, X/Y, tekerlek, yatay kaydırma |
| 0x05 | NKRO klavye (120 tuşluk bitmap) |
| 0x08 | Medya tuşları (Consumer, 16-bit) |
| 0x09 | **Windows Dial** (Surface Dial uyumlu döner kontrol). Bu klavyenin **topuzu**. |
| 0x11 | Eject ve Apple Fn (Mac modu) |
| — | **Üreticiye özel (0xFF00 ve üstü) bir arayüz YOK.** Bluetooth üzerinden ışık, tuş ve makro ayarı **yapılamaz**. |

---

## 2. Model özellikleri [A]

- **Model:** AJAZZ AK980 PRO, ekranlı üç modlu sürüm ("AK980 三模带屏版"). BT adları `AK980 PRO-1/-2/-3`.
- **Düzen:** %98 düzen, 97 tuş ve metal topuz.
- **Ekran:** 1.14" TFT, 240×135 piksel.
- **Gövde ve tuşlar:** gasket montaj, FR4 plaka, 3 ve 5 pinli hot-swap.
- **Aydınlatma:** güney yönlü RGB, 20 efekt, tuş başına renk (per-key), yan ışık şeridi.
- **Pil:** 8000 mAh. **Tuş algılama:** NKRO.
- **Bağlantı:** USB-C, 2.4G alıcı ve Bluetooth. Bluetooth sürümü kılavuzlarda 5.0 ve 5.1 olarak çelişiyor.
  - Mod seçimi ve Win/Mac seçimi **fiziksel sürgü anahtarlarla** yapılır.
- **Tarama hızı (polling):** kablo ve 2.4G'de 1000 Hz, Bluetooth'ta 125 Hz.
- **Çipler:** ana çip SONiX ailesinden (SN32F2xx ya da HFD80CP100 klonu, doğrulanmadı). Bluetooth ve 2.4G için WCH CH58x.
- **Resmi yazılım:** "AK980 Triple Mode With Screen RGB Keyboard Driver" v1.0.0.6. Yalnızca Windows için bir MFC uygulaması (`DeviceDriver.exe`).
  - Bu model için resmi web sürücüsü yok.
  - VIA/QMK ve OpenRGB desteği yok. SignalRGB listesinde var.

## 3. Klavye üzerindeki Fn kombinasyonları [A]

| Kombinasyon | İşlev |
|---|---|
| Fn + \ | Işık efektini değiştir (20 efekt) |
| Fn + ↑ / ↓ | Parlaklık (6 kademe) |
| Fn + ← | Efekt yönü |
| Fn + → | Renk: RGB, kırmızı, turuncu, sarı, yeşil, camgöbeği, mavi, mor, beyaz |
| Fn + - / = | Hız (6 kademe) |
| Fn + X | Işıkları aç / kapat |
| Fn + Z | Yan RGB şeridini aç / kapat |
| Fn + 1 / 2 / 3 | Özel oyun ışık setleri (FPS / LOL / ofis) |
| Fn + ~ | Tuş başına renk kaydı |
| Fn + Q / W / E | BT kanal 1–3. Kısa basış geçiş yapar, 3 s basılı tutmak eşleştirme başlatır. |
| Fn + R (3 s) | 2.4G eşleştirme |
| Fn + Win | Win tuşu kilidi |
| Fn + Space (3–5 s) | **Fabrika ayarı** (BT eşleştirmeleri de silinir) |
| Fn + F1–F12 | Medya ve kısayol tuşları |
| Topuz | Çevirince ses düzeyi, basınca sessiz |
| Fn + topuz | Ekran menüsü |
| Fn + Del | Ekranı aç / kapat |
| Fn + Home | Ekran animasyonu |
| Fn + C / V | Ekran dili (Çince / İngilizce) |

## 4. Ayar protokolü [A, doğrulanmalı]

> **Güncel not (2026-11-10):** Bu bölüm ilk araştırma özetidir. Cihazda ölçülen düzeltmeler için `docs/donanim-dogrulama.md` dosyasına bakın. Başlıcaları:
> - Işık kuyruğu `AA 55`.
> - `04 F5` sayacı `09` olmalı.
> - Yankı durumu `00` da kabul anlamına gelir; yalnızca `FF` ret demek.
> - `04 02` yankısı bir sağlama toplamı taşır.

### USB kimlikleri

| Mod | VID:PID | Kontrol arayüzü |
|---|---|---|
| Kablolu (USB) | `0C45:8009` ("SONiX AK980 PRO") | MI_03, usage page **0xFF13**: komutlar. MI_02, usage page **0xFF68**: ekran verisi. |
| 2.4G alıcı | `0C45:FEFE` | MI_03, usage page **0xFF60** (kısıtlı komut seti) |
| Bluetooth | `0C45:FEFE` (BLE PnP) | Yok. Sadece pil okunabilir. |

> PID 8009, AK820 Pro ile ortak. Klavyeyi tanımak için ürün adına ve tuş düzenine bakılmalı.

### Kablolu mod iletişimi

- Paketler 64 bayttır. Başlarına rapor ID 0 eklenir ve `HidD_SetFeature` ile gönderilir.
- Her yazmanın ardından `HidD_GetFeature` ile yankı (echo) okunur. Paketler arasında yaklaşık 35 ms beklenir.
- **Bir işlemin akışı:**
  1. `04 18` ile başlatılır.
  2. `04 OP ARG 0… COUNT_LO COUNT_HI` başlığı gönderilir.
  3. COUNT kadar veri paketi gönderilir.
  4. `04 02` ile kaydedilir.
  5. Işık ayarlarında ayrıca `04 F0` ile uygulanır.

### Komut kodları

| OP | İşlev |
|---|---|
| `13` | Efekt ayarı: `MODE R G B 0 0 0 0 RAINBOW BRIGHT(0-5) SPEED(0-5) DIR 0 0 AA 55`. Mod 1–19, 0 kapalı, 0x80 tuş başına renk. |
| `23` | Tuş başına renk: 9 paket, `[idx R G B] × 144` |
| `F5` | Renkleri geri okuma |
| `11` / `27` | Temel / Fn katmanı tuş haritası (576 bayt) |
| `19` / `15` | Makrolar |
| `17` | Seçenekler (uyku süresi, Win kilidi vb.) |
| `28` | Saat senkronizasyonu |
| `72` | Ekrana resim/GIF yükleme (0xFF68 üzerinden, 4 KiB sayfalar, RGB565) |

### 2.4G alıcı iletişimi

- Raporlar 33 bayttır; son bayt, baytların toplamından oluşan sağlama değeridir.
- Pil: `20 01`. Işık: `05 10 00…`. Saat: `0C 10…`.
- Tuş başına renk ve ekran yükleme bu modda **yok**.

> Kaynaklar arasında mod numaraları ve bazı komut kodları çelişiyor. Gerçek değerler, resmi sürücünün USB trafiği **USBPcap + Wireshark** ile kaydedilerek doğrulanmalı.
>
> Örnek bir çelişki: ak820pro-modder'a göre ışık verisi 14–15. baytlarda `55 AA` ile bitiyor, saat ve uyku verisi ise 62–63. baytlarda `AA 55` ile. Yukarıdaki tabloda ışık için `AA 55` yazıyor; bu da doğrulanmalı.

### ak820pro-modder ile ilişkisi

- **Repo:** https://github.com/wsclx/ak820pro-modder (MIT lisanslı). Tauri 2 (Rust + React) ile yazılmış.
- **Hedefi farklı:**
  - Klavye: AK820 Pro (%75 düzen, 0.85" 128×128 ekran).
  - İşletim sistemi: yalnızca macOS. Windows sürümü yok.
- **Protokolü bizimkiyle aynı:** Repodaki "Family B" ailesi, AK980 PRO'nun kablolu modunda beklenen protokolle birebir örtüşüyor:
  - Komutlar 0xFF13 üzerinden feature report olarak gidiyor, ekran verisi 0xFF68 üzerinden.
  - İşlem sırası: `18 → veri → 02 → F0`, paketler arasında 35 ms bekleme.
  - Bu yüzden `crates/ak820-protocol` kütüphanesi doğrudan temel alınabilir.
- **AK980 PRO'ya uyarlanması gerekenler:**
  - Klavyeyi tanıma: PID 8009 ürün adına bakılmadan kabul ediliyor, yani AK980 bir AK820 sanılır. "AK980" ürün adı kontrolü eklenmeli.
  - Ekran: 128×128 sabit kodlanmış. 240×135'e çevrilmeli.
  - Tuş başına renk: indeks tablosu AK820 ANSI düzenine göre. 98 tuşluk düzene göre yeniden çıkarılmalı.
  - Family B'de tuş haritası ve makro protokolü çözülmemiş. Bunu biz çözeceğiz.
  - macOS'a özel parçalar (AppleScript, iCloud, ScreenCaptureKit) Windows karşılıklarıyla değiştirilmeli: WASAPI loopback ses yakalama, Windows Media Session, PowerShell veya AutoHotkey.

## 5. Güvenlik kuralları (cihazı bozmamak için)

1. **0xFEE0 / 0xFEE1 (BLE OTA) servisine asla yazılmayacak.**
2. Yazılım güncelleme ve bootloader komutları uygulamaya **hiç eklenmeyecek**.
3. Komutlar şu sırayla devreye alınacak:
   1. Salt-okunur komutlar (pil, renk geri okuma)
   2. Işık komutları (`13`, `23`)
   3. Seçenekler
   4. Tuş haritası ve makrolar (en son)
4. Bilinmeyen bir komut kodu, trafiği kaydedilip anlaşılmadan gönderilmeyecek.
5. Her yazmadan önce mevcut ayarlar yedeklenecek. Kurtarma yolu olarak Fn+Space ile fabrika ayarına dönülebilir.

## 6. Kaynaklar

- Resmi kılavuz (PDF): https://download.s21i.co99.net/25789609/0/0/ABUIABA9GAAglJjoygYo7q2vjAU.pdf
- Resmi sürücü v1.0.0.6: https://www.a-jazz.com/en/h-col-159.html
- C# yeniden yazım (en iyi protokol referansı): https://github.com/donglover69/sonix-keyboard-studio
- Linux / AK980 V2 notları: https://github.com/YellowHearth1/ajazz-ak980-v2-linux
- Protokol notları: https://github.com/Aiacos/ajazz-control-center/blob/main/docs/protocols/keyboard/ak980pro_vendor.md
- Kardeş model AK820 Pro donanımı: https://github.com/fpb/ajazz-ak820-pro
- Aynı BLE modülünün analizi (AK35i): https://github.com/Ike-li/ak35i-v3-max-macos-control-center/blob/main/docs/STATIC_REVERSING.md

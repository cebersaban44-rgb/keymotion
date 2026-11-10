# Mimari Karar Belgesi (ADR-001): Uygulama yığını ve katmanlar

- **Durum:** Kabul edildi (2026-11-10, Tauri 2 + Rust). Uygulanan hali için bkz. `docs/DURUM.md` §4.
- **Tarih:** 2026-11-10
- **Kapsam:** Ürün kalitesinde (AA+) bir klavye kontrol uygulaması.
  - İlk hedef: Windows ve AJAZZ AK980 PRO.
  - Sonraki hedef: benzer AJAZZ, AULA, Epomaker ve SONiX tabanlı modeller.

---

## 1. Soru

> "Her şeyi Rust ile yazmak daha mantıklı değil mi? Neden ekstra bir çalıştırma aracı gerekiyor? İki ayrı yapı yerine tek yapı olsun."

Bu belge, saf Rust ile Rust çekirdek + web arayüz (Tauri) seçeneklerini bu ürünün ihtiyaçlarına göre karşılaştırır.

## 2. Önce bir yanlış anlamayı düzeltelim: Tauri'de "ekstra çalıştırma aracı" yok

| | Geliştirme sırasında | Kullanıcının bilgisayarında |
|---|---|---|
| Node.js / npm | Arayüz dosyalarını derlemek için kullanılır (Vite) | **Yok.** Derlenen HTML/CSS/JS dosyaları `.exe`'nin içine gömülür. |
| Rust | Derleyici | Tek bir `.exe` dosyası (yaklaşık 5–10 MB) |
| WebView2 | — | **Windows 11'de hazır gelir.** Edge'in motoru, Windows Update ile güncellenir. |

Tauri uygulaması **tek bir Rust programıdır.** Arayüzü çizmek için işletim sisteminin kendi tarayıcı motorunu (WebView2) kullanır. Electron gibi kendi Chromium'unu ve Node'unu paketlemez.

**Gerçek maliyetler** şunlardır; bunları saklamıyoruz:

1. **İki dil:** Rust ve TypeScript.
2. **Bir iletişim sınırı:** Arayüz ile Rust arasında JSON taşınır (IPC).
3. **Pencere açıkken ek süreçler:** WebView2, `msedgewebview2.exe` süreçleri açar. Bunlar toplam yaklaşık 60–120 MB bellek kullanır.

## 3. Seçenekler

| Seçenek | Arayüz nasıl çizilir | Dil | Arayüz kalitesi tavanı | Hazır bileşen ekosistemi | Bellek (pencere açık) | Olgunluk |
|---|---|---|---|---|---|---|
| **A. Tauri 2 + TypeScript** | WebView2 | Rust + TS | Çok yüksek | Çok geniş | ~80–150 MB | Kararlı (v2, 2024'ten beri) |
| B. Tauri 2 + Leptos | WebView2 (WASM) | Sadece Rust | Yüksek | Dar | ~80–150 MB | Orta |
| **C. Slint** | Kendi çizim motoru (Skia veya FemtoVG) | Rust + `.slint` arayüz dili | Yüksek, ama bileşenleri biz yazarız | Dar | ~15–40 MB | Kararlı (1.x) |
| D. iced | wgpu | Sadece Rust | Orta–yüksek | Dar | ~30–60 MB | 1.0 öncesi, API değişiyor |
| E. egui | wgpu (anlık çizim) | Sadece Rust | Orta; geliştirici aracı gibi görünür | Orta | ~30–50 MB | Kararlı, ama tüketici ürünü için zor |
| F. Dioxus Desktop | WebView (Tauri gibi) | Sadece Rust | Yüksek | Dar | ~80–150 MB | Orta; kendi çizim motoru Blitz henüz alfa |
| G. WinUI 3 (windows-rs ile) | Yerel XAML | Rust | Yüksek (Windows görünümü) | — | ~40–80 MB | Rust için resmi destek yok, pratikte zor |

Bellek değerleri yaklaşıktır, ölçüm değildir.

**Eleme gerekçeleri:**

- **B ve F:** "Saf Rust" diye tercih edilse bile yine WebView kullanırlar. Üstelik web ekosisteminin hazır bileşenlerini de kaybederler; iki dünyanın kötü yanlarını birleştirirler.
- **D ve E:** Tüketici ürünü cilasına ulaşmak çok emek ister.
- **G:** Rust için desteklenmiyor.

Gerçek finalistler **A (Tauri + TS)** ve **C (Slint)**.

## 4. Bu ürünün arayüzden beklediği

Bu uygulama bir form ekranı değil, yoğun görsel düzenleyiciler içeriyor:

- **Tuş başına renk boyama:** 98 tuşlu, ölçekli klavye görseli. Fırça, kova ve degrade araçları, katmanlar, canlı önizleme.
- **Renk seçici:** HSV tekerleği, HEX/RGB girişi, palet, son kullanılan renkler.
- **Tuş atama:** Sürükle-bırak, aranabilir eylem kataloğu (tuşlar, medya, fare, makro, uygulama başlatma), Fn katmanı.
- **Makro kaydedici:** Zaman çizelgesi ve gecikme düzenleme.
- **Ekran (240×135 TFT) için GIF stüdyosu:** Kırpma, kare zamanlaması, RGB565 önizleme.
- **Ses tepkili ışık:** Spektrum görselleştirme.
- **Genel kalite:** Akıcı animasyonlar, açık/koyu tema, Windows 11 görünümü (Mica), Türkçe/İngilizce, erişilebilirlik.

## 5. Karar: **A, yani Tauri 2. Ama "Rust öncelikli" kurallarla.**

### Neden

1. **Arayüz kalitesi ve hız.** Yukarıdaki düzenleyicilerin hepsinin web dünyasında olgun, test edilmiş karşılıkları var (renk seçiciler, sürükle-bırak, canvas, animasyon kütüphaneleri). Slint'te renk tekerleği, zaman çizelgesi ve sürükle-bırak katmanlarını sıfırdan yazmak gerekir. Aynı kaliteye ulaşmak birkaç kat zaman alır.
2. **Sektördeki örnekler.** Bu kategorideki en iyi ürünler web teknolojisiyle arayüz yapıyor: Wooting Wootility, Keychron Launcher, VIA, NuPhy Console, SteelSeries GG. Bu bir tesadüf değil.
3. **Stratejik avantaj: tarayıcıdan da çalışabilir.** Protokol çekirdeği saf Rust olacağı için WebAssembly'ye derlenebilir. Aynı arayüz, WebHID ile tarayıcıdan da çalışır. Kurulumsuz bir web yapılandırıcı ve Mac/Linux'a erişim sağlar. Bu yalnızca web arayüzle mümkün.
4. **"İki yapı" maliyetini küçültüyoruz:**
   - **Tüm mantık Rust'ta.** Protokol, cihaz modelleri, profiller, efekt motoru, ses analizi ve arka plan servisi Rust'ta olur. TypeScript yalnızca görüntü katmanıdır, iş mantığı içermez.
   - **Tipler tek kaynaktan gelir.** TypeScript tipleri Rust'tan otomatik üretilir (`tauri-specta`). Elle iki kez yazılan model olmaz, uyumsuzluk derleme anında yakalanır.
   - **Arka planda düşük bellek.** Pencere kapanınca WebView tamamen yok edilir. Sistem tepsisinde yalnızca Rust süreci kalır, birkaç MB ile. Bu uygulama zamanının %95'ini tepside geçirir.
5. **Arayüz değiştirilebilir kalır.** Çekirdek, arayüzden bağımsız crate'lerden oluşur. İleride Slint'e geçmek istersek yalnızca `app` katmanı değişir.

### Ne zaman saf Rust (Slint) doğru olurdu

- Hedef sadece ayar ekranları olsaydı ve gelişmiş düzenleyiciler olmasaydı.
- Bellek ve başlatma süresi bir numaralı öncelik olsaydı (gömülü cihazlar, çok zayıf donanım).
- Tek dil zorunluluğu kalite ve hızdan daha önemli olsaydı.

Bunlar bu ürün için geçerli değil. Yine de çekirdeği arayüzden bağımsız tuttuğumuz için bu kapı açık kalıyor.

## 6. Katmanlar (Cargo workspace)

```
klavye/
├─ crates/
│  ├─ kb-transport/   HID (hidapi): feature/output/input raporları, zamanlama, yeniden deneme
│  │                  BLE (windows-rs WinRT): pil okuma ve bildirimleri (salt-okunur)
│  ├─ kb-protocol/    Saf ve G/Ç'siz paket kodlama/çözme. Protokol aileleri:
│  │                    sonix_ff13 (AK980 PRO kablolu), sonix_dongle, sonix_ff68 ...
│  │                  WASM'a derlenebilir. Gerçek USB kayıtlarından alınan test örnekleriyle test edilir.
│  ├─ kb-devices/     Model tanımları (veri dosyaları): VID/PID, ürün adı, arayüzler,
│  │                  fiziksel tuş düzeni, LED indeksleri, ekran boyutu, efekt listesi, yetenekler
│  ├─ kb-core/        Üst seviye işlemler: ışık, tuş başına renk, tuş haritası, makro, ekran (RGB565/GIF),
│  │                  profiller, yedekle-yaz-doğrula, flash aşınma koruması
│  ├─ kb-service/     Arka plan: cihaz takılma/çıkarma izleme, tepsi, uygulamaya göre profil,
│  │                  saat senkronu, pil izleme, ses tepkili ışık (WASAPI), canlı efekt motoru
│  └─ kb-cli/         Geliştirici ve test aracı: listeleme, sondalama, kayıttan tekrar oynatma
├─ app/
│  ├─ src-tauri/      İnce kabuk: Tauri komutları ve olayları, pencere ve tepsi yaşam döngüsü
│  └─ ui/             TypeScript + React (yalnızca görüntü), tipler Rust'tan üretilir
└─ docs/
```

**Model ekleme kuralları:**

- **Yeni bir model eklemek** = `kb-devices` altına bir veri dosyası eklemek. Gerekirse `kb-protocol` içine yeni bir protokol ailesi yazılır.
- Her protokol ailesi bir `trait KeyboardDriver` uygular.
- Arayüz, yetenek bayraklarına göre ekranları açar veya gizler. Örneğin dongle modunda tuş başına renk sekmesi pasif olur.

## 7. Cihaz güvenliği (ürün kalitesinin parçası)

1. **Asla yazılmayacaklar:** Firmware ve bootloader komutları, BLE OTA kanalı (`0xFEE0/0xFEE1`).
2. **Bilinmeyen komut gönderilmez.** Her komut önce resmi sürücünün USB kaydıyla doğrulanır ve test örneğine dönüştürülür.
3. **Flash aşınması:** Bu ailede ışık ayarları kalıcı hafızaya (flash) yazılıyor. Bu yüzden:
   - Kaydırıcı sürüklenirken flash'a yazılmaz.
   - Canlı önizleme için RAM'e yazan bir "gerçek zamanlı ışık" komutu aranacak. Resmi sürücüde "real-time" ve "music" ışık modları var; bu, böyle bir komutun var olduğuna işaret ediyor.
4. **Yedekle → yaz → geri oku → doğrula** sırası izlenir. Hata olursa yedek geri yüklenir. Son çare olarak Fn+Space ile fabrika ayarı anlatılır.

## 8. Ürün kalitesi gereksinimleri

- **CI:** `cargo fmt`, `clippy -D warnings`, birim ve kayıt-tekrar testleri, arayüz tip denetimi, uçtan uca testler.
- **Dağıtım:** Kod imzalama (Authenticode). Olmazsa SmartScreen uyarı verir. MSI veya NSIS kurulum dosyası, imzalı otomatik güncelleme.
- **Kullanıcı deneyimi:** Türkçe ve İngilizce, erişilebilirlik (klavyeyle gezinme, ekran okuyucu), açık/koyu tema ve Windows 11 Mica.
- **Gizlilik:** Çökme raporu yalnızca kullanıcı izniyle (opt-in). Telemetri varsayılan olarak kapalı.

## 9. Referans kaynak politikası

Topluluk projeleri (ak820pro-modder, sonix-keyboard-studio vb.) **kopyalanmaz, temel alınmaz.** Kullanım şekli:

- Protokol bilgisi oradan **referans** olarak alınır ve kendi kodumuzla yazılır.
- Her bilgi bizim cihazımızda doğrulanır.
- MIT lisanslı kaynaklar `docs/` içinde ve "Hakkında" ekranında anılır.

Resmi sürücü paketindeki model XML dosyaları (tuş düzeni, LED indeksleri) yalnızca **veri kaynağı** olarak incelenir. Bu dosyalar dağıtılmaz.

## 10. Yol haritası

| Faz | İçerik | Bitti sayılması için |
|---|---|---|
| 0 | Workspace, `kb-transport`, `kb-cli list/probe`, BLE pil okuma | CLI klavyeyi üç modda da tanıyor |
| 1 | Resmi sürücünün USB trafiğini her özellik için kaydetme; protokol belgesi ve test örnekleri | Her komut bir kayıtla doğrulanmış |
| 2 | Işık: 20 efekt, renk, parlaklık, hız, yön; tuş başına renk; yan şerit | Klavyede görsel olarak doğrulanmış |
| 3 | Tuş atama (Temel ve Fn katmanı), makrolar, seçenekler (uyku, Win kilidi) | Yedekle-yaz-doğrula ile |
| 4 | Ekran: resim/GIF yükleme, saat; topuz | 240×135 doğru gösteriliyor |
| 5 | Arka plan servisi: tepsi, uygulamaya göre profil, ses tepkili ışık, pil uyarısı | Pencere kapalıyken düşük bellek |
| 6 | Ürünleştirme: imzalama, kurulum, güncelleme, i18n, erişilebilirlik | Sürüm adayı |
| 7 | Yeni modeller (AK820 Pro, AK980 V2, AULA F108 Pro ...) | Model başına veri dosyası ve test |

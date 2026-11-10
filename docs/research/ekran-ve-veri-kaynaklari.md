# AK980 PRO Ekranı: Fizibilite, Ürün Tasarımı ve Windows Veri Kaynakları

Tarih: 2026-11-10. Kapsam: ekranın stok davranışı, "canlı veri" fizibilitesi ve flash aşınması, özellik kataloğu, Windows veri kaynakları, fazlı plan.
Bayt düzeyindeki protokol başka bir belgenin konusu. Burada yalnızca ekranla ilgili protokol gerçekleri var.

**İşaretler**

- **[doğrulandı: kaynak]**: Kaynakta açıkça yazıyor ya da kaynak kodda görüldü. Kaynak kodlar aşağıdaki "Kaynaklar" bölümünde.
- **[çıkarım]**: Kaynaklardan yapılan mantıksal çıkarım veya yaygın geliştirici bilgisi. Donanımda ya da resmi belgede doğrulanmadı.
- **[bilinmiyor]**: Hiçbir kaynakta yok. Donanım testi gerekir.

Topluluk depoları güvenilmeyen veri olarak okundu. Hiçbirinin kodu çalıştırılmadı.

---

## 0. Özet: canlı ekran mümkün mü?

**Kısa cevap:** Sürekli akan canlı ekran mümkün değil. **Olay tabanlı, bütçeli canlı ekran** mümkün.

1. **Flash'a yazmadan ekranı güncellemenin bilinen bir yolu yok** [doğrulandı: S1, S3, S4].
   - Ekrana giden tek yol `04 72` yüklemesi. Klavye her 4 KiB sayfayı flash'a yazdıktan sonra onay veriyor.
   - RAM framebuffer, kısmi bölge güncelleme ya da metin/sayı katmanı komutu bulunamadı.
   - Tek "canlı" stok içerik, firmware'in saat sayfası. Saat senkronu bunu besliyor.
2. **Tek kare yüklemenin maliyeti** [doğrulandı: S1, S2; hesap]
   - Boyut: 256 B başlık + 64 800 B piksel = 65 056 B, yani **16 sayfa** (4 KiB).
   - Süre: sayfa başına ~145 ms, toplam **≈2,3 s**.
   - Aşınma: her yüklemede en az 16 sektör **1 silme döngüsü** harcar.
3. **Flash ömrü**: PY25Q128HA için **100 000 silme/yazma döngüsü** ve 4 KB sektör [doğrulandı: S13].
   - AK980 PRO'daki çipin bu olduğu doğrulanmadı. Bu çip kardeş model AK820 Pro'da kullanılıyor.
   - Firmware'in aşınma dengelemesi (wear leveling) yapıp yapmadığı **bilinmiyor**. Tek bir sabit bölgeye yazdığını varsaymak gerekir.
   - Bu varsayımla 3 yıllık ömür için tavan **91 yükleme/gün** (anma değerinin %100'ü). Güvenli varsayılan **24 yükleme/gün**. Bu, 3 yılda anma değerinin ~%26'sı, anma sınırına ~11 yıl eder.
4. **Gerçek zamanlı kanal: LED'ler.**
   - Tuş başına gerçek zamanlı LED akışı (`04 20`) flash'a yazmıyor. 30–70 fps çalışıyor [doğrulandı: S1, AK980 V2 üzerinde ölçülmüş].
   - Sık değişen veri (CPU yükü, ses düzeyi, bildirim, Pomodoro ilerlemesi, ses spektrumu) LED'lere gider.
   - Ekrana ise "bir sonraki olaya kadar geçerli" bağlam gider: şarkı, sıradaki toplantı, odak durumu.

---

## 1. Model kimliği ve kaynakların geçerliliği

- **AK980 PRO, topluluğun "AK980 V2" dediği modelle aynı aileden.**
  - Kimlikler aynı: `0C45:8009` (kablolu) ve `0C45:FEFE` (2.4G alıcı).
  - Ekran aynı: 240×135, en çok 140 kare.
  - Resmi araç aynı: `DeviceDriver.exe`.
  - [doğrulandı: S14 (bu PC'de BT PnP ID `0C45:FEFE`, USB ürün adı "SONiX AK980 PRO"), S4 (`config.xml`: `gif_maxframes="140" gif_count="1" width="240" height="135"`), S1, S3]
  - Ad "PRO" ya da "V2" olabilir. Bunun aynı donanım revizyonu olduğu **[çıkarım]**.
- **AK820 Pro kaynakları yalnızca kıyas için kullanılabilir.**
  - AK820 Pro'da iki protokol ailesi var.
  - **Aile A** (web sürücüsü, `0xFF67` üzerinde `AA 50`): firmware 1.16'da yüklemeler **yalnız RAM'de** tutuluyor [doğrulandı: S7].
  - **Aile B** (`0xFF13` + `0xFF68`, 4 KiB sayfalar): yüklemeler flash'a yazılıyor [doğrulandı: S6, S8].
  - **AK980 PRO, Aile B yapısını kullanıyor** [doğrulandı: S1, S3, S5]. Yani AK820 Aile A'daki "ücretsiz canlı ekran" burada yok.
- `fraustiz/freejazz`, ekransız AK980 Max (`0C45:8043`) için yazılmış. Ekran açısından ilgisiz. Windows'ta Rust ile canlı LED efekti ve ses yakalama örneği olarak yararlı [doğrulandı: S10].

---

## 2. (A) Stok ekran: kutudan çıktığı gibi

### 2.1 Sayfalar ve menü

| Özellik | Durum |
|---|---|
| Ana sayfa | Saat, tarih, bağlantı modu, pil düzeyi, ışık durumu [doğrulandı: S11, S12] |
| Özel GIF sayfası | Var. Sürücüyle yükleniyor [doğrulandı: S11, S1] |
| Yerleşik animasyonlar | `Fn + Home` ile açılıyor [doğrulandı: S14 (kılavuz)]. Sayısı **bilinmiyor**. |
| Topuz menüsü | Topuz varsayılan olarak ses düzeyi ve sessiz. `Fn + topuz` menüyü açar, ya da topuzu ses/menü modu arasında değiştirir [doğrulandı: S11, S3, S14]. |
| Menü öğeleri | Connectivity, System, Backlight Brightness/Speed, RGB Modes/Colors, Volume, dil seçimi [doğrulandı: S11, S12] |
| Dil | Çince / İngilizce, `Fn + C / V` ile [doğrulandı: S14] |
| Ekranı aç / kapat | `Fn + Del` [doğrulandı: S14, S3] |
| Caps / Num Lock göstergesi | Ekranda gösterildiği hiçbir kaynakta yok. Num Lock vurgusunu firmware LED'lerde çiziyor [doğrulandı: S3]. Ekran için **bilinmiyor**. |
| Ses düzeyi göstergesi | Menüde "Volume" öğesi var. Topuz çevrilince ekranda gösterge çıkıp çıkmadığı **bilinmiyor**. |
| Saat | Gerçek zaman saati, ana bilgisayardan senkronlanıyor (`04 28`; alıcı üzerinden `0C 10`). Her saat paketi ekranı saat sayfasına geçiriyor [doğrulandı: S1, AK980 V2'de 2026-10-30 testi]. |
| Bluetooth modunda topuz | Windows Dial olarak görünüyor (HID rapor `0x09`) [doğrulandı: S14 (bu PC)] |

### 2.2 Özel içerik sınırları

| Konu | Değer |
|---|---|
| Çözünürlük ve biçim | 240×135, RGB565 little-endian, satırlar yukarıdan aşağı, 64 800 B/kare [doğrulandı: S1, S3, S4] |
| Slot sayısı | **1.** `gif_count="1"` [doğrulandı: S4]. Slot 2'ye yükleme slot 1'deki animasyonu sildi. Topuz menüsünde yalnız yeni animasyon ve saat sayfası kaldı [doğrulandı: S1]. |
| En çok kare | 140 [doğrulandı: S1, S3, S4] |
| Kare süresi | 2 ms birimle 1–255, yani 2–510 ms [doğrulandı: S1, S2] |
| Resmi aracın süre sınırı | AK820 Aile B'de resmi araç süreyi 30–500 ms ile sınırlıyor [doğrulandı: S8]. AK980 için **bilinmiyor**. |
| Gerçek en kısa kare süresi | AK820 Aile A (fw 1.16) ekranı ~60 ms/kareden hızlı çizemiyor [doğrulandı: S7]. AK980 için **bilinmiyor**, kronometreyle ölçülmeli. |
| Animasyon sayfasına geri dönüş | Animasyonu yeniden yüklemeden geçiren bilinen komut yok. Saat sayfasından animasyona dönmek = yeniden yükleme [doğrulandı: S1]. Kullanıcı topuz menüsünden elle seçebilir. |
| Geri okuma | Ekrandaki animasyonu okuyan komut yok [doğrulandı: S1, S7]. Uygulama kendi kopyasını saklamalı. |
| Resmi aracın yan etkisi | Stok uygulama her açılışta saati ve bir sayfa numarasını yeniden gönderiyor. Bu yüzden ekran sayfa değiştiriyor [doğrulandı: S3]. |

### 2.3 Yükleme süresi (sayfa başına ~145 ms)

Hesap: sayfa sayısı = ⌈(256 + 64 800·N) / 4096⌉. Süre ≈ sayfa × 0,145 s [ölçüm: S1, S2; hesap].

| Kare (N) | Sayfa | Süre | Flash'ta kapladığı alan |
|---:|---:|---:|---:|
| 1 | 16 | ≈2,3 s | 64 KiB |
| 2 | 32 | ≈4,6 s | 128 KiB |
| 4 | 64 | ≈9,3 s | 256 KiB |
| 10 | 159 | ≈23 s | 0,62 MiB |
| 20 | 317 | ≈46 s | 1,24 MiB |
| 28 | 444 | ≈64 s (S1'de ölçülen) | 1,73 MiB |
| 60 | 950 | ≈2,3 dk | 3,71 MiB |
| 140 | 2 215 | ≈5,4 dk | 8,65 MiB |

### 2.4 Bağlantı modlarına göre ekran

| Mod | Durum |
|---|---|
| Kablolu | Yükleme, saat ve pil (dolaylı) çalışıyor [doğrulandı: S1, S3, S5] |
| 2.4G alıcı | Saat senkronu ve pil sorgusu çalışıyor [doğrulandı: S1 (AK980 V2'de test edildi)]. |
| 2.4G'de ekran yüklemesi | Topluluk araçları "yok" diyor [doğrulandı: S1, S3, S5]. Resmi sürücüde `7F 03` başlıklı, 28 baytlık parçalarla bir yol var [doğrulandı: S4 (decompile), S1]. Ama parça düzeni çözülmedi ve donanımda hiç denenmedi. Çalışsa bile kare başına ≥4,6 s sürer [S4 hesabı] ve yine flash'a yazar **[çıkarım]**. |
| Bluetooth | Ayar kanalı yok. Üreticiye özel arayüz bulunmuyor [doğrulandı: S14, S4 (`1033.lan` id 62)]. |
| Kablolu moddan çıkınca | Klavye 2.4G moduna geçince kablolu USB arayüzleri kayboluyor. Kablo yalnızca şarj ediyor [doğrulandı: S1, S3]. |

### 2.5 Yükleme sırasında gözlenenler

- Bir satıcı yorumuna göre GIF yüklerken tüm ışıklar sönüyor [bildirildi: tek kaynak, arama sonucu].
- Yükleme sırasında yazmanın sürüp sürmediği **bilinmiyor**. YellowHearth'ın hatırlatıcı özelliği kullanıcı çalışırken arka planda yükleme yapıyor. Bu, yazmanın sürdüğüne işaret ediyor **[çıkarım]**.
- AK820 Aile B'de gözlenenler [doğrulandı: S8]:
  - "Alındı" onayı "kaydedildi" anlamına gelmiyor.
  - Hızlı gönderimde şeritli ya da kaymış görüntü oluşabiliyor.
  - Her komuttan önce 35 ms beklemek gerekiyor.
- AK820'de bir kullanıcı, kısa bir yüklemenin arkasında eski karelerin pikselleri kaldığını bildirdi [doğrulandı: S6 board-scan §3.1, 0x5k]. AK980'de **bilinmiyor**. Test edilmeli.

---

## 3. (B) Canlı veri fizibilitesi

### 3.1 Flash'a yazmadan güncelleme yolları

| Aday yol | AK980 PRO durumu |
|---|---|
| RAM framebuffer / akış modu | **Yok.** Hiçbir kaynakta bu tür bir komut geçmiyor [doğrulandı: S1, S3, S4]. AK820 Aile A'da `0x50` yüklemeleri RAM'de duruyor [S7]. Ama bu komut ailesi AK980'de yok. |
| Kısmi bölge güncelleme | **Yok.** Blob her zaman başlıkla başlıyor. Adres ya da ofset alanı yok [doğrulandı: S1, S2]. |
| Metin/sayı katmanı (istatistik sayfası) | AK820 web sürücüsünde var (cmd 52, `setTftScreenInfo`: CPU %, °C, GPU, hava kodu, nem). Ama yalnız `isShowTftInfo` bayrağı olan modellerde açık [doğrulandı: S6 board-scan §2.3, S7]. AK980'in Windows sürücüsünün ekran menüsünde istatistik özelliği yok. Orada yalnız GIF düzenleme, "Time Syns" ve yükleme var [doğrulandı: S4 `1033.lan` 800–887]. **AK980 için bilinmiyor; muhtemelen yok [çıkarım].** |
| Saat sayfası | **Var.** Firmware gerçek zaman saatini kendisi çiziyor. Senkron sık gerekmez. Paketin flash'a bir şey yazıp yazmadığı **bilinmiyor**. AK820 Pro'da saat ayrı bir RTC çipinde (D8563F) [doğrulandı: S6 board-scan §3.3]. AK980'de **bilinmiyor**. |
| Gerçek zamanlı LED akışı (`04 20`) | **Var, flash'a yazmaz.** 30–70 fps [doğrulandı: S1]. Kare, bir sonraki kareye ya da yapılandırma komutuna kadar kalıyor [S1]. Yalnız kabloyla çalışıyor. |

### 3.2 ak820pro-modder "Now Playing" nasıl yapıyor

[doğrulandı: S6 `src-tauri/src/now_playing_tft.rs`, `tft_memory.rs`, README]

- macOS'ta Music.app ve Spotify'ı **2 sn'de bir** JXA ile sorguluyor.
- İmza `(kaynak, çalıyor mu, başlık, sanatçı)`. İmza değişmedikçe yükleme yapmıyor. Yani yükleme **şarkı değiştiğinde** oluyor, yoklama 2 sn.
- Kare 128×128. Metin ana bilgisayarda `embedded-graphics` ASCII mono fontlarıyla çiziliyor (`FONT_8X13_BOLD`, `FONT_6X10`, `FONT_5X8`). Albüm kapağı yok. Türkçe karakter çizemez.
- Yükleme başarısız olursa bir sonraki turda yeniden deniyor.
- `TftMemory` modülü, ışık değişince firmware'in ekranı sıfırlamasına karşı son içeriği 750 ms gecikmeyle yeniden yüklüyor.
- **Önemli emsal:** Aynı proje flash kullanan **Aile B kartlarda Now Playing'i ve otomatik yeniden yüklemeyi tamamen kapatıyor.** README'deki ifade: Aile B'de "uygulama asla kendi başına yazmaz: canlı önizleme yok, ekranın otomatik yeniden yüklenmesi yok" [doğrulandı: S6 README, MACOS-800A.md].
- Başka bir fork (adamauthor) günde 100–300 ışık yazımı ve ~90 otomatik görsel yüklemesi yapan özellikleri flash endişesiyle kaldırdı. Onun özeti: "a few writes per day, not per second" [doğrulandı: S6 board-scan §4.1].

### 3.3 Flash aşınma hesabı

**Bilinenler**

- PY25Q128HA (AK820 Pro'da) [doğrulandı: S13]:
  - 128 Mbit (16 MB), 4 KB tekdüze sektör, 256 B sayfa.
  - 100 000 silme/yazma döngüsü, 20 yıl veri saklama.
  - 4 KB sektör silme tipik 50 ms. Sayfa yazma 0,5 ms ile 2,4 ms arasında. Bu iki değer belgede çelişiyor.
- AK980'de her 4 KiB sayfanın onayı ~145 ms ve klavye onayı "flash'a yazdıktan sonra" veriyor [doğrulandı: S1, S3]. Bu süre "sektör silme + 16 sayfa yazma + ek yük" ile uyumlu **[çıkarım]**.
  - AK820 Aile B'de ilk blok ~130 ms, sonrakiler ~38 ms [doğrulandı: S8].
  - 16 × 2,4 ms = 38 ms. Bu, AK820'de silmenin başta toplu yapıldığını düşündürüyor **[çıkarım]**.
  - İki durumda da **her yükleme, kapsadığı her sektörü 1 kez siler.**

**Bilinmeyenler**

- AK980 PRO'daki flash çipinin markası ve dayanıklılığı.
- Firmware animasyonu her seferinde aynı adrese mi yazıyor, slotlar arasında dönüyor mu? Tek slot ve "slot 2 slot 1'i ezdi" gözlemi sabit bölgeyi düşündürüyor **[çıkarım]**.
- Yüklemede ayrıca bir ayar/metadata sektörü güncelleniyor mu?
- Saat senkronu (`04 28` + `04 02`) flash'a yazıyor mu?
- ak820pro-modder README'sinde "flash: firmware, settings, display frames" yazıyor. Firmware'in SPI flash'ta mı MCU'nun içinde mi durduğu doğrulanmadı.

**Kilit gözlem [çıkarım]:** Sektör başına aşınma, **kare sayısına değil yükleme sayısına** bağlı.

- 1 karelik ve 30 karelik yüklemenin her ikisi de ilk 16 sektörü 1 kez siler.
- Uzun animasyon daha fazla sektöre dokunur ama her birine yine 1 kez.
- Sonuç: bütçeyi "günlük yükleme sayısı" ile tutmak doğru. Kare sayısı yalnızca süreyi belirler.

**Bütçe tablosu** (en kötü durum: aşınma dengeleme yok, sabit bölge, 100k döngü)

| Hedef | Anma değerinin %100'ü | %50'si | %30'u | %25'i |
|---|---:|---:|---:|---:|
| 3 yıl | 91/gün | 46/gün | 27/gün | 23/gün |
| 5 yıl | 55/gün | 27/gün | 16/gün | 14/gün |

| Günlük yükleme | 100k sınırına kadar | 30k'ya kadar (güvenli bölge) |
|---:|---:|---:|
| 8 | 34 yıl | 10 yıl |
| 24 | 11,4 yıl | 3,4 yıl |
| 40 | 6,8 yıl | 2,1 yıl |
| 60 | 4,6 yıl | 1,4 yıl |
| 100 | 2,7 yıl | 0,8 yıl |
| 150 | 1,8 yıl | 0,5 yıl |

**Örnek senaryo:** Her şarkı değişiminde yükleme yapan Now Playing.

- Günde 3 saat müzik ve ortalama 3,5 dk şarkı süresi ≈ 51 yükleme/gün. Bu, 100k sınırına ~5,4 yıl eder.
- Günde 8 saat müzik ≈ 137 yükleme/gün. Bu, ~2 yıl eder.
- Ek kısıt olmadan, yoğun kullanıcıda ürün ömrünü tüketir.

Ek not: Aile B'de **ışık ve tuş ayarı yazımları da flash'a kaydediliyor** [doğrulandı: S6 README, MACOS-800A.md]. Uygulama başına ışık profili değiştirmek gibi özellikler aynı bütçeye dahil edilmeli.

### 3.4 Önerilen güvenli ürün tasarımı

**İlke:** Ekran = olay tabanlı bağlam. LED = gerçek zamanlı veri. Saat = firmware.

1. **Aşınma bütçesi (token bucket)**
   - Varsayılan **"Dengeli": 24 yükleme/gün**, anlık en çok 6 yükleme. Saatlik dolum = bütçe / 24.
   - **"Tasarruf": 8/gün.**
   - **"Yoğun": 48/gün.** Seçilince açık bir uyarı gösterilir.
   - Kullanıcının elle yaptığı yüklemeler bütçeyi tüketir ama engellenmez. Yalnızca bilgi verilir.
2. **Aşınma ölçer (UI)**
   - Cihaz başına kalıcı bir sayaç tutulur. Sayılanlar: ekran yüklemeleri ve flash'a kaydeden tüm komutlar (ışık, tuş haritası, makro). Saat senkronu da muhafazakâr olarak sayılabilir.
   - Gösterilenler: "Bugün 7/24 güncelleme", "Tahmini ekran belleği ömrü kullanımı: %0,4 (100 000 döngü varsayımı)".
   - Değerlerin tahmini olduğu yazılır: firmware'in iç davranışı bilinmiyor, başka PC'lerde ya da araçlarla yapılan yazımlar sayılamıyor.
   - Toplam 30 000'i geçince "Tasarruf" moduna otomatik düşülür ve kullanıcıya bildirilir.
3. **Yalnızca olay tabanlı güncelleme. Her kaynak için bekleme kuralı:**
   - **Now Playing:** Şarkı ≥10–15 s çaldıktan sonra yükle. Böylece hızlı atlamalar yüklenmez. Duraklatılmışsa yükleme.
   - **Takvim:** "Sıradaki etkinlik" değiştiğinde.
   - **Pomodoro:** Faz değiştiğinde.
   - **Hava:** En sık 3 saatte bir. Yalnızca durum kategorisi ya da ±3 °C değiştiyse.
   - **Uygulama profili:** Uygulama ≥60 s önde kalırsa ve o uygulamaya ekran profili atanmışsa.
4. **Birleştirme ve tekilleştirme**
   - Tek bir "bekleyen hedef kare" tutulur. Yükleme sürerken yeni durum gelirse bekleyen kare değiştirilir: en son gelen kazanır.
   - Gönderilecek RGB565 blob'un özeti (hash) alınır. Son yüklenenle aynıysa yüklenmez.
   - Bu özet diske yazılır. Böylece uygulama yeniden başlayınca aynı içerik tekrar yüklenmez.
5. **Bayat değer yasağı**
   - Görsele saat, pil yüzdesi, CPU yüzdesi gibi hızla eskiyen değerler basılmaz.
   - Görsel, bir sonraki olaya kadar doğru kalan içerik taşır. Örnek: "Sıradaki: 14:00 Standup". "5 dk sonra" gibi göreli zaman kullanılmaz.
6. **Hızlı veri LED'e gider**
   - CPU/GPU barı rakam sırasına, ses düzeyi barı F sırasına basılır.
   - Bildirimde çift yanıp sönme, mikrofon kapalıyken kırmızı tuş, Pomodoro ilerlemesi, ses spektrumu da LED'lerde gösterilir.
   - Hepsi `04 20` gerçek zamanlı akışıyla, ana bilgisayarda birleştirilerek gönderilir. YellowHearth ve freejazz böyle yapıyor [doğrulandı: S1, S10].
   - Uygulama kapanınca kayıtlı efekte dönmek için bir ışık komutu gerekebilir. Bu da flash'a yazar. Oturum başına 1 yazım kabul edilebilir. Kayıtsız dönüş yolu protokol belgesinde araştırılmalı **[bilinmiyor]**.
7. **Zamanla ilgili içerik için saat sayfası**
   - Saat ve tarih için hiç yükleme yapılmaz. Senkron bağlanınca ve haftada bir yapılır, saat dilimi değişince hemen yapılır. YellowHearth da böyle yapıyor [S1].
   - Uyarı: her saat paketi ekranı saat sayfasına çeker. Kullanıcı görsel sayfasındayken senkron yapılmaz, ya da yapılacağı söylenir.
   - **Deneysel "odak kronometresi":** Odak başında klavye saati 00:00:00'a ayarlanır. Saat sayfası geçen süreyi yükleme yapmadan canlı gösterir. Bitince gerçek saat geri yüklenir.
     - Geri sayım yapılamaz, yalnız ileri sayım.
     - Saat paketinin flash'a yazıp yazmadığı ve saniye gösterilip gösterilmediği test edilmeli **[çıkarım, test gerekli]**.
8. **Önceden çizilmiş döngü ("pano döngüsü")**
   - Birden çok kart tek animasyon olarak yüklenebilir. Örneğin hava + takvim + pil, her kart 5 sn.
   - Kare süresi en çok 510 ms. Bu yüzden 5 sn için ~10 aynı kare gerekir. 3 kart ≈ 30 kare ≈ 69 s yükleme.
   - Aşınma tek yükleme kadar. Saatte bir yapılırsa günde 24 yükleme eder: bütçenin tamamı.
   - Bu nedenle ileri bir faz için ve isteğe bağlı.
9. **Canlı mod yalnızca uygulama açıkken.** Kapanışta iki seçenek sunulur: son statik görseli bırakmak ya da saat sayfasına dönmek (saat senkronuyla, yükleme gerekmez).
10. **2.4G ve Bluetooth modu:** Ekran güncellenemez. Yalnız en son durum kuyrukta tutulur. Kabloya dönülünce bütçe izin veriyorsa tek yükleme yapılır.
11. **Yükleme UX'i**
    - İlerleme çubuğu ve kalan süre gösterilir (sayfa × 0,145 s).
    - Yükleme sırasında LED akışı duraklatılır. Tek bir HID kilidi kullanılır.
    - Yükleme ortasında iptal yok. Yarım animasyon kalabilir [çıkarım; S8'deki "loading" durumu].
    - Kullanıcının kendi yaptığı yüklemelerde onay sorulur. YellowHearth da böyle yapıyor [S1 DESIGN.md].
12. **Faz 0 donanım testleri (ürün kararlarından önce):**
    - Sayfa süresi.
    - En kısa kare süresi.
    - Yükleme sırasında yazma ve LED davranışı.
    - Kısa yüklemenin eski kareleri bırakıp bırakmadığı.
    - Saat paketinin sayfa ve saniye davranışı.
    - `04 20` sonrası kayıt yazmadan efekte dönüş.
    - 24 saatlik tekrar yükleme dayanıklılık testi. Bu yalnızca ayrılmış bir test cihazında yapılır.

---

## 4. (C) Özellik kataloğu

Yük: günlük tipik ekran yüklemesi. "LED" = gerçek zamanlı akış (flash'a yazmaz).

| Özellik | Değer | Ekranda fizibilite | Güncelleme stratejisi | Yük/gün | Önerilen kanal |
|---|---|---|---|---:|---|
| **Görsel/GIF stüdyosu** | Çok yüksek (temel özellik) | Tam | Elle, tek seferlik. Kırpma, 140 kareye inceltme, 2 ms'ye yuvarlama, dithering, RGB565 önizleme, süre ve aşınma tahmini | 0–2 | Ekran |
| **Now Playing** (kapak + başlık + sanatçı) | Çok yüksek | Olay tabanlı | Şarkı ≥10–15 s çaldıysa. Bütçe dolunca durur. Seçenekler: "her şarkı", "albüm değişince", "kısayolla sabitle" | 10–40 | Ekran + LED spektrum |
| **Saat / tarih / dünya saati** | Yüksek | Firmware ile, bedava | Saat senkronu. Dünya saati = klavye saatini başka dilime ayarlamak | 0 | Saat sayfası |
| **Pomodoro / zamanlayıcı** | Yüksek | Olay tabanlı | Faz başında "Odak 25:00" / "Mola" kartı. İlerleme LED barıyla. İsteğe bağlı RTC kronometre hilesi | 8–16 | Ekran + LED |
| **Takvim: sıradaki etkinlik** | Yüksek | Olay tabanlı | Sıradaki etkinlik değişince. Mutlak saatle yaz | 3–10 | Ekran |
| **Özel metin / kayan yazı** | Orta | Tek seferlik | Statik metin ucuz. Kayan yazı önceden çizilir (20 kare ≈ 46 s yükleme) | 0–2 | Ekran |
| **Oyun oturumu özeti** | Orta–yüksek | Olay tabanlı | Oyun açılınca oyun kartı, kapanınca özet ("2s13dk, GPU ort. %87, en çok 74 °C") | 2–6 | Ekran + LED |
| **Uygulama başına ekran profili** | Orta | Riskli | Yalnız atanmış uygulamalar. ≥60 s bekleme. Alt+Tab gürültüsü filtrelenir. Işık profili LED akışıyla yapılır, flash'a kaydedilmez | 2–10 | Ekran + LED |
| **Hava** | Orta | Kısıtlı | En sık 3 saatte bir. Kategori ya da ±3 °C değişince | 2–6 | Ekran |
| **Sistem istatistikleri** (CPU/GPU/RAM/sıcaklık/ağ) | Yüksek (oyuncu) | **Uygun değil** (sürekli değişiyor) | LED barları. Ekranda yalnız istek üzerine anlık görüntü ya da oyun özeti | 0–2 | **LED** |
| **Bildirim sayısı** | Düşük–orta | Uygun değil | LED'de yanıp sönme. Sayı ekranda gösterilmez | 0 | LED |
| **Mikrofon sessiz durumu** | Orta | Uygun değil (toplantıda sık değişir) | Sessizken bir tuş kırmızı | 0 | LED |
| **Ses düzeyi** (topuz çevrilince) | Orta | Uygun değil (2,3 s gecikme, aşınma) | F sırasında 1,5 s bar | 0 | LED |
| **Pil / bağlantı** | Düşük | Firmware zaten gösteriyor | — | 0 | Firmware |
| **Pano döngüsü** (çok kartlı) | Orta | Pahalı | Saatlik tek yükleme. ~60–70 s sürer | ≤24 | Ekran (ileri faz) |

---

## 5. (D) Windows veri kaynakları: Rust yaklaşımı ve tuzaklar

Sürümler crates.io'dan 2026-11-10'da alındı.

Genel not: Çalışma alanında `unsafe_code = "deny"` var (`C:\dev\klavye\Cargo.toml`). windows-rs'te Win32/COM çağrılarının çoğu `unsafe`. Bu yüzden bu çağrılar ayrı bir platform crate'inde, örneğin `kb-platform-win` içinde, `#![allow(unsafe_code)]` ile ve güvenli sarmalayıcılarla toplanmalı **[çıkarım]**. Çoğu WinRT çağrısı unsafe değil.

### 5.1 Now Playing ve albüm kapağı

- **API:** `windows` 0.62.x, özellikler `Media_Control`, `Foundation`, `Storage_Streams`.
  - `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()` ile yönetici alınır, ardından `GetCurrentSession()` ve `GetSessions()`.
  - Yönetici olayları: `CurrentSessionChanged`, `SessionsChanged`.
  - Oturumda `TryGetMediaPropertiesAsync()` şunları verir: `Title`, `Artist`, `AlbumTitle`, `AlbumArtist`, `Thumbnail`.
  - Oturum olayları: `MediaPropertiesChanged`, `PlaybackInfoChanged` (`GetPlaybackInfo().PlaybackStatus()`), `TimelinePropertiesChanged`.
  - Uygulama kimliği `SourceAppUserModelId` ile bulunur.
  - Gereksinim: Windows 10 1809 (17763), UniversalApiContract v7 [doğrulandı: W1].
- **Kapak:** `Thumbnail()` bir `IRandomAccessStreamReference` döner.
  - Okuma: `OpenReadAsync()`, sonra `DataReader::CreateDataReader`, `LoadAsync(size)`, `ReadBytes`.
  - Çözme: `image::load_from_memory`.
- **Tuzaklar**
  - Belgede `globalMediaControl` yeteneği listeleniyor [doğrulandı: W1]. Paketlenmemiş (unpackaged) Win32 uygulamalarının bu API'yi sorunsuz kullanabildiği topluluk pratiği **[çıkarım]**. İlk prototipte doğrulanmalı.
  - Özellikler parça parça gelir: önce başlık, sonra kapak. `MediaPropertiesChanged` birkaç kez tetiklenir. Render'dan önce 1–2 s bekle **[çıkarım]**.
  - Kapak olmayabilir ya da düşük çözünürlüklü olabilir. Tarayıcı oturumlarında alanlar boş gelebilir [doğrulandı: Chromium hata kaydı (arama sonucu); albüm alanı geçmiyor].
  - `GetCurrentSession` duraklatılmış bir tarayıcı sekmesini seçebilir. "Çalan oturumu tercih et" kuralı yazılmalı **[çıkarım]**.
  - Olay işleyicileri sistem iş parçacığında çalışır. İşleyicide yalnız kanala mesaj gönder. Oturum değişince eski abonelik token'larını kaldır, yenisine abone ol.
  - Explorer yeniden başlarsa ya da hata alınırsa yönetici yeniden istenmeli **[çıkarım]**.

### 5.2 CPU, RAM ve ağ

- **`sysinfo` 0.40.1**
  - CPU: `System::refresh_cpu_usage()` iki ölçüm arasında `MINIMUM_CPU_UPDATE_INTERVAL` ister.
  - RAM: `refresh_memory()`.
  - Ağ: `Networks::new_with_refreshed_list()` ve `refresh()` ile `received()` / `transmitted()` farkları.
  - Yalnız gereken özellikleri aç. freejazz örneği `default-features = false, features = ["system"]` kullanıyor [doğrulandı: S10].
- **Hafif alternatifler**
  - CPU: `GetSystemTimes` (idle/kernel/user farkı).
  - RAM: `GlobalMemoryStatusEx`.
  - Ağ: `GetIfTable2` (`Win32::NetworkManagement::IpHelper`).
  - Performans sayaçları: PDH (`Win32::System::Performance`: `PdhOpenQueryW`, `PdhAddEnglishCounterW`, `PdhCollectQueryData`, `PdhGetFormattedCounterValue`).
- **Tuzak: kullanıcı değeri Görev Yöneticisi ile karşılaştırır.**
  - Eski yapılarda Görev Yöneticisi "% Processor Utility" kullanıyordu. Bu değer frekansa göre ölçeklenir ve %100'ü aşabilir.
  - Windows 11 24H2 (KB5058411, Mayıs 2025) ile zamana dayalı tek formüle geçildi [doğrulandı: W14].
  - LED barı için `\Processor Information(_Total)\% Processor Time` ya da `GetSystemTimes` yeterli.

### 5.3 GPU kullanımı ve sıcaklığı

- **NVIDIA:** `nvml-wrapper` 0.13.0.
  - Kullanım: `Nvml::init()`, `device_by_index(0)`, `utilization_rates()`, `temperature(TemperatureSensor::Gpu)`, `memory_info()`.
  - `nvml.dll` sürücüyle gelir ve dinamik yüklenir. NVIDIA olmayan makinede zarifçe devre dışı kalır.
  - Tuzak: Optimus dizüstülerde sorgu uyuyan dGPU'yu uyandırıp pil tüketebilir **[çıkarım]**. Yalnızca dGPU etkinken sorgula.
- **AMD:** ADLX bir C++ SDK. Rust `adlx` crate'i yalnızca `0.0.0-alpha.1` (2024) [doğrulandı: crates.io]. bindgen ya da eski ADL (`atiadlxx.dll`) gerekir. MVP'de satıcıdan bağımsız yol önerilir.
- **Satıcıdan bağımsız kullanım:** PDH sayaçları.
  - `\GPU Engine(*)\Utilization Percentage`: örnek adları `pid_…_luid_…_engtype_3D`. LUID ve motor türüne göre topla, motor türlerinin en büyüğünü al. Bu, Görev Yöneticisi'ne yakın bir değer verir **[çıkarım]**.
  - `\GPU Adapter Memory(*)\Dedicated Usage`.
  - Tuzaklar: Örnekler süreçlerle birlikte doğup ölür. Joker karakterli sorgu ve `PdhGetFormattedCounterArrayW` gerekir. İlk değer için iki toplama gerekir.
- **Satıcıdan bağımsız sıcaklık:** `D3DKMT_ADAPTER_PERFDATA.Temperature`, birimi 0,1 °C [doğrulandı: W10].
  - windows-rs'te `Wdk::Graphics::Direct3D` altında [doğrulandı: W10].
  - Erişim: `D3DKMTQueryAdapterInfo` ya da `D3DKMT_QUERYSTATISTICS_PHYSICAL_ADAPTER_INFORMATION`. Sorgu türünün adı ve WDDM sürüm şartı test edilmeli **[bilinmiyor]**.
- **DXGI tuzağı:** `IDXGIAdapter3::QueryVideoMemoryInfo` → `CurrentUsage` **yalnız çağıran uygulamanın** kullanımını verir, sistem toplamını değil [doğrulandı: W11]. Sistem VRAM'i için PDH ya da NVML kullan.

### 5.4 CPU sıcaklığı: Windows'ta neden zor

- Paket ya da çekirdek sıcaklığı MSR ve SMN kayıtlarından okunur. Bunun için **çekirdek sürücüsü** gerekir. Kullanıcı modunda belgelenmiş bir API yok **[çıkarım; LHM'nin ring0 sürücü gereksinimi]**.
- **WMI `MSAcpi_ThermalZoneTemperature` (`root\WMI`)**
  - `sysinfo` Windows'ta bunu kullanıyor [doğrulandı: W18 kaynak kodu].
  - Yönetici hakkı ister. ACPI bölge sıcaklığıdır, CPU kalıbının değil. Birçok anakartta sabit değer döner ya da hiç dönmez **[çıkarım; yaygın bilgi]**.
- **WinRing0**
  - Microsoft Defender Mart 2025'ten beri CVE-2020-14979 nedeniyle WinRing0'ı "HackTool:Win32/Winring0" / "VulnerableDriver:WinNT/Winring0" olarak işaretliyor. Fan Control, OpenRGB ve eski LibreHardwareMonitor sürümleri etkilendi [doğrulandı: W7].
  - **Asla paketlenmemeli.**
- **LibreHardwareMonitor**
  - v0.9.5 (07 Ocak) "Swap WinRing0 to PawnIO" (#1857) değişikliğini getirdi. Kütüphane PawnIO'yu doğrudan kullanıyor (#1908). Ring0 kurulumunu kapatma seçeneği var (#1777) [doğrulandı: W8].
  - v0.9.6 (14 Şubat): PawnIO modülleri 2.2 [doğrulandı: W8].
  - PawnIO ayrı ve yönetici haklarıyla kurulmalı. Kurulmazsa CPU sıcaklığı ve anakart sensörleri okunamıyor [doğrulandı: OhmGraphite notları (arama sonucu)].
- **Önerilen yol: isteğe bağlı "harici sensör sağlayıcı".**
  - (a) **HWiNFO paylaşılan belleği.** Ücretsiz sürümde 12 saat sınırı var: süre dolunca SHM kapanıyor ve elle yeniden açılması gerekiyor [doğrulandı: W9]. Nesne adının `Global\HWiNFO_SENS_SM2` olduğu **[çıkarım]**.
  - (b) **LHM çalışırken WMI** (`root\LibreHardwareMonitor`) ya da uzak web sunucusu (`data.json`) **[çıkarım; README'de doğrulanamadı]**.
  - Sağlayıcı yoksa CPU sıcaklığı UI'da gizlenir.
  - Kendi çekirdek sürücümüz ya da PawnIO kurulumu MVP'de **yok**.

### 5.5 Bildirimler

- `UserNotificationListener` (`Windows.UI.Notifications.Management`)
  - Manifestte `userNotificationListener` yeteneği gerekir [doğrulandı: W2].
  - `RequestAccessAsync` UI iş parçacığından çağrılmalı [doğrulandı: W2].
  - Kullanıcı izni şart. İzin geri alınırsa API'ler sessizce boş döner [doğrulandı: W2].
  - Yalnız toast bildirimleri desteklenir. Değişiklikler `NotificationChanged` olayı ya da `UserNotificationChangedTrigger` arka plan görevi ile alınır [doğrulandı: W2].
- **Paket kimliği gerekir.** MSIX ya da "packaging with external location" (seyrek paket, imzalı) kullanılmalı [doğrulandı: W3]. Tauri'nin NSIS/MSI kurulumları paketlenmemiş uygulama üretir. Seyrek paket ve kod imzalama sertifikası gerekir **[çıkarım]**.
- **Öneri:** MVP'de yok. Faz 3'te isteğe bağlı. Yalnız "bildirim geldi" bilgisi LED'de yanıp söner. İçerik okunmaz, gizlilik vurgulanır.
- Bildirim veritabanı (`wpndatabase.db`) belgelenmemiş ve kilitli. **Kullanılmamalı.**

### 5.6 Takvim

- **`Windows.ApplicationModel.Appointments`** (`AppointmentManager::RequestStoreAsync(AllCalendarsReadOnly)`)
  - `appointments` yeteneği, yani paket kimliği gerekir **[çıkarım]**.
  - Windows Posta ve Takvim uygulamalarının desteği **31 Aralık 2024'te bitti** [doğrulandı: W4]. Yeni Outlook'un bu depoyu doldurduğuna dair bilgi yok. Depo büyük olasılıkla boş **[çıkarım]**.
- **Önerilen:** **ICS URL'si.**
  - Kaynaklar: Google "iCal biçiminde gizli adres", Outlook "takvimi yayımla" bağlantısı.
  - Ayrıştırma: `icalendar` 0.17 (parser). Tekrarlanan etkinlikler (RRULE) ve saat dilimleri için ek iş gerekir. `rrule` crate'i önerilir **[çıkarım]**.
  - Sorgulama 15–30 dk'da bir. Ekran yalnızca "sıradaki etkinlik" değişince güncellenir.
- **İleri faz:** Microsoft Graph ya da Google Calendar API, OAuth (PKCE + loopback) ile. Yalnızca not.

### 5.7 Hava

- **Open-Meteo**
  - Anahtar gerekmiyor.
  - Ücretsiz kullanım **yalnızca ticari olmayan** projeler için. Abonelik ya da reklam içeren uygulama ticari sayılıyor.
  - Sınırlar: günde <10 000, saatte 5 000, dakikada 600 çağrı. CC BY 4.0 atfı zorunlu. Ticari kullanım ücretli plan gerektiriyor [doğrulandı: W5].
  - **Ürün satılacaksa ya da reklam içerecekse ücretli plan gerekir.**
- **Alternatif: MET Norway (`api.met.no`)**
  - CC BY 4.0. Kimlik bildiren `User-Agent` zorunlu. Saniyede <20 istek.
  - `Expires` ve `If-Modified-Since` başlıklarına uyulmalı. Koordinatlar en çok 4 ondalık [doğrulandı: W6].
  - Ticari kullanım ToS'ta ayrıca belirtilmemiş, lisans sayfasına yönlendiriliyor.
- **Konum:** Kullanıcı şehir girer (Open-Meteo geocoding). Konum servisi izni gerekmez. Koordinat 2 ondalığa yuvarlanır (gizlilik).
- **Ekranda:** En sık 3 saatte bir. Simgeler `resvg` ile çizilir. "Weather data by Open-Meteo.com" atfı uygulamanın "Hakkında" bölümüne konur. Ekranda yer yok.

### 5.8 Ses: loopback, seviye ölçer, ses düzeyi

- **Loopback**
  - `cpal` 0.18.2: bir **çıkış cihazı giriş akışı olarak açılırsa** WASAPI loopback otomatik devreye girer [doğrulandı: W13 (cpal kaynak yorumu), S10 (freejazz bunu Windows'ta kullanıyor)].
  - Daha fazla denetim için `wasapi` 0.27.0 (HEnquist).
  - Microsoft'a göre loopback yalnız paylaşımlı (shared) modda çalışır. Olay güdümlü loopback Windows 10 1703 ve sonrasında var [doğrulandı: W13].
  - FFT için `rustfft` 6.4 ya da `realfft` 3.5.
- **Tepe seviyesi:** `IAudioMeterInformation::GetPeakValue`.
  - Yol: `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)` → `Activate::<IAudioMeterInformation>`.
  - Yakalama akışı gerektirmez, çok ucuz. VU barı için yeterli **[çıkarım]**.
- **Ses düzeyi**
  - `IAudioEndpointVolume::GetMasterVolumeLevelScalar` / `GetMute`.
  - Değişiklik bildirimi: `RegisterControlChangeNotify`. `IAudioEndpointVolumeCallback` windows-rs `#[implement]` ile yazılır.
  - Varsayılan cihaz değişirse `IMMNotificationClient::OnDefaultDeviceChanged` ile yeniden bağlanılır.
  - Topuz çevrilince gösterge LED barıyla verilir, ekranla değil.
- **Bluetooth modunda topuz Windows Dial** [doğrulandı: S14]. `RadialController` için `IRadialControllerInterop::CreateForWindow(HWND)` kullanılabilir. İsteğe bağlı **[çıkarım]**.

### 5.9 Mikrofon sessiz durumu (Teams / Discord)

- **Teams:** Yerel WebSocket "üçüncü taraf uygulama API'si" (port 8124, "Manage API") kullanımdan kaldırılıyor. Elgato Stream Deck entegrasyonu bu yüzden bitti, Microsoft duyurusu MC1266901 [doğrulandı: W15]. **Kullanılmamalı.**
- **Discord RPC:** `rpc.voice.read` kapsamı Discord onayı gerektiriyor [doğrulandı: W16]. Pratikte kullanılamaz.
- **Satıcıdan bağımsız yol**
  - Varsayılan kayıt cihazında `IAudioEndpointVolume::GetMute` ve geri çağrı. Bu, sistem düzeyindeki sessizdir.
  - Uygulama içindeki sessiz düğmesi işletim sistemine yansımaz **[çıkarım]**.
  - "Mikrofon kullanımda" bilgisi: `HKCU\...\CapabilityAccessManager\ConsentStore\microphone` altında `LastUsedTimeStop == 0` **[çıkarım; topluluk yöntemi]**.
  - Gösterim yalnız LED'de.

### 5.10 Ön plandaki uygulama (uygulama başına profil)

- **Olay kancası:** `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, cb, 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)`.
  - Kancayı kuran iş parçacığında bir mesaj döngüsü (`GetMessageW`) gerekir.
  - Ayrıca `EVENT_SYSTEM_MINIMIZEEND` dinlenmeli.
- **Süreç adı:** `GetWindowThreadProcessId` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → `QueryFullProcessImageNameW`.
- **Tuzaklar [çıkarım]**
  - UWP uygulamaları `ApplicationFrameHost.exe` olarak görünür. Asıl süreç alt pencereden (`CoreWindow`) bulunmalı.
  - Korumalı ya da hile önleyicili süreçlerde `OpenProcess` reddedilebilir. Pencere sınıfı ya da başlığa dönülür.
  - Alt+Tab gürültüsünü süzmek için ≥60 s bekleme (ekran) ve ≥1 s bekleme (LED profili) uygulanır.
  - Tam ekran oyunda odak değişimleri olabilir.

### 5.11 240×135 RGB565 çizim hattı

- **Tuval:** `tiny-skia` 0.12, `Pixmap::new(240, 135)`. Yollar, gradyanlar, kenar yumuşatma.
- **Simgeler:** `resvg` 0.49 (+`usvg`). SVG doğrudan tiny-skia Pixmap'ine çizilir (hava ve uygulama simgeleri).
- **Metin**
  - **`cosmic-text` 0.19:** Şekillendirme, iki yönlü metin, satır kaydırma. `fontdb` ile **Windows sistem fontlarından yedek** kullanır (Segoe UI, Yu Gothic, Malgun vb.). Japonca/Korece/Arapça şarkı adları için önerilir. Rasterleştirme `swash` 0.2 ile.
  - **`fontdue` 0.9.5 / `ab_glyph` 0.2.32:** Basit, hızlı. Şekillendirme ve yedek font yok. Sabit Latin metin için yeterli. YellowHearth `ab_glyph` kullanıyor [doğrulandı: S2].
- **Türkçe tuzakları**
  - ak820pro-modder'ın `embedded-graphics` ASCII fontları (`ascii::FONT_6X10` vb.) ğ, ş, ı, İ çizemez [doğrulandı: S6 kodu].
  - Inter, Noto Sans ve Segoe UI Latin Extended-A'yı kapsar **[çıkarım]**.
  - Rust'ın `to_uppercase()` fonksiyonu yerel ayardan bağımsızdır: "i" → "I" olur, "İ" olmaz. Türkçe başlıklar için kendi tr-büyük/küçük harf fonksiyonumuz yazılmalı **[çıkarım; std davranışı]**.
  - Emoji için renkli font yedeği ya da filtre gerekir.
- **Okunabilirlik**
  - Ekran 1,14", ~241 ppi.
  - En küçük okunur yazı ~11–12 px. 1 px'ten ince çizgi kullanılmaz. Metin piksele hizalanır **[çıkarım]**.
- **Görsel girdi**
  - `image` 0.25.10: PNG, JPEG, WebP, GIF, APNG. `into_frames()` GIF karelerini birleştirerek çözer.
  - `gif` 0.14.2: kare düzeyinde denetim gerekirse.
  - Boyutlandırma: Lanczos3. Seçenekler: sığdır, doldur, uzat, yakınlaştır/kaydır.
- **RGB565 ve dithering**
  - Fotoğraf ve gradyanda **sıralı Bayer 4×4** kullanılır. Metin ve UI kartlarında dithering **kapalı**. YellowHearth bu ayrımı kodunda yapıyor [doğrulandı: S2].
  - Animasyonda Floyd–Steinberg kareden kareye "kaynayan" gürültü üretir. Sıralı dithering tercih edilir **[çıkarım]**.
  - UI önizlemesi gerçek 16 bit renkleri göstermeli. sonix-keyboard-studio böyle yapıyor [doğrulandı: S3].
- **Kare kuralları**
  - Süre 2 ms birimle, 1–255 aralığına sıkıştırılır. 140 kareyi aşan animasyon inceltilir.
  - UI'da yükleme süresi gösterilir: ⌈(256+64 800·N)/4096⌉ × 0,145 s.

### 5.12 Cihaz tak-çıkar algılama

- **`CM_Register_Notification`** (Windows 8+)
  - Filtre: `CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE` + `GUID_DEVINTERFACE_HID`.
  - Geri çağrıda engelleyici iş yapılmaz. İş ayrı bir iş parçacığına aktarılır.
  - Mevcut cihazları bildirmez: önce kaydol, sonra listele [doğrulandı: W12].
  - windows-rs yolu: `Win32::Devices::DeviceAndDriverInstallation`.
  - `RegisterDeviceNotification` pencere tanıtıcısı ister. UI yaşam döngüsüne bağlanmamak için CM_* tercih edilir.
- **`hidapi` 2.6.7:** Olaydan sonra 300–500 ms bekle, sonra `HidApi::refresh_devices()`. Aynı cihazın birkaç koleksiyonu ayrı ayrı gelir **[çıkarım]**.
  - Filtre: VID `0x0C45`, PID `0x8009` veya `0xFEFE`.
  - Usage page: `0xFF13` (komut), `0xFF68` (ekran), `0xFF60` (alıcı) [doğrulandı: S1, S3].
- **Mod anahtarı:** Klavye 2.4G'ye geçince kablolu arayüzler kaybolur [doğrulandı: S1, S3]. UI yetenekleri anında güncellenir ve ekran kuyruğa alınır.
- **Bayat tanıtıcı:** Fiş çekilince hidapi tanıtıcısı ölü kalır. Bağlantı hatasında tanıtıcı temizlenip bir kez yeniden açılır [doğrulandı: S6 HANDOFF §6.9f].

### 5.13 Bluetooth LE pil

- Bu PC'de doğrulandı: `0x180F` okunuyor ve bildirim (notify) destekliyor (%94). `0x1812` HID servisi Windows tarafından kilitli. `0xFEE0`/`0xFEE1` OTA kanalına **asla yazılmaz** [doğrulandı: S14].
- **WinRT yolu**
  - Workspace'te `Devices_Bluetooth*` özellikleri zaten açık.
  - Akış: `BluetoothLEDevice::FromBluetoothAddressAsync` → `GetGattServicesForUuidAsync(GattServiceUuids::Battery())` → `GetCharacteristicsForUuidAsync(GattCharacteristicUuids::BatteryLevel())` → `ReadValueAsync(BluetoothCacheMode::Uncached)`.
  - Bildirim için: `WriteClientCharacteristicConfigurationDescriptorAsync(Notify)` + `ValueChanged`.
  - `ConnectionStatusChanged` olayıyla yeniden abone olunur.
  - Bazı cihazlarda okuma eşleştirme ister [doğrulandı: W17 (Microsoft blog)].
- **Alternatif (oturum açmadan):** Windows Ayarlar'ın gösterdiği PnP özelliği `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` (bayt, yüzde) [doğrulandı: topluluk W17; resmi belge yok].
  - Okuma: `CM_Get_DevNode_PropertyW` ya da `DeviceInformation` ek özelliği **[çıkarım]**.
- 2.4G'de alıcı üzerinden `20 01` pil sorgusu çalışıyor [doğrulandı: S1]. Kablolu modda doğrudan okunamıyor [doğrulandı: S1, S6].

### 5.14 Crate özeti

| Amaç | Crate / API | Sürüm (2026-11-10) |
|---|---|---|
| Win32 / WinRT | `windows` (`Media_Control`, `Win32_Media_Audio`, `Win32_Devices_DeviceAndDriverInstallation`, `Win32_System_Performance`, `Wdk_Graphics_Direct3D` …) | 0.62.2 |
| Sistem metrikleri | `sysinfo` | 0.40.1 |
| NVIDIA | `nvml-wrapper` | 0.13.0 |
| Ses | `cpal` / `wasapi` / `realfft` | 0.18.2 / 0.27.0 / 3.5.1 |
| HID | `hidapi` | 2.6.7 |
| Çizim | `tiny-skia` / `resvg` | 0.12.0 / 0.49.0 |
| Metin | `cosmic-text` (+`swash`) / `fontdue` / `ab_glyph` | 0.19.0 / 0.9.5 / 0.2.32 |
| Görsel | `image` / `gif` | 0.25.10 / 0.14.2 |
| Takvim | `icalendar` | 0.17.17 |
| HTTP | `reqwest` | 0.13.5 |
| WMI (isteğe bağlı) | `wmi` | 0.18.4 |
| AMD | `adlx` | yalnızca 0.0.0-alpha.1. **Önerilmez.** |

---

## 6. Önerilen fazlı plan

### Faz 0: Donanım doğrulama (1–2 gün, test cihazında)

1. **Ölçümler**
   - Tek karelik yükleme süresi ve sayfa süresi.
   - En kısa gerçek kare süresi (kronometre).
   - Yükleme sırasında yazma ve LED davranışı.
   - Kısa yükleme eski kareleri bırakıyor mu?
2. **Saat paketi**
   - Sayfa geçişi.
   - Saniye gösteriliyor mu?
   - "00:00 kronometre" hilesi çalışıyor mu?
3. **`04 20` canlı LED akışı**
   - Başlatma ve durdurma.
   - Kayıt yazmadan efekte dönülebiliyor mu?
4. Bu sonuçlarla §2 ve §3'teki **[bilinmiyor]** satırları güncellenir.

### Faz 1: MVP "Ekran Stüdyosu"

- **GIF/görsel stüdyosu**
  - İçe aktar, kırp, sığdır/doldur.
  - Kare süresi, inceltme, metin katmanı.
  - Bayer dithering, gerçek RGB565 önizleme.
  - Süre ve aşınma tahmini, onaylı yükleme.
  - Yerel geçmiş: ekran geri okunamadığı için kopya saklanır.
- **Saat senkronu:** Bağlanınca (isteğe bağlı), haftalık ve saat dilimi değişince.
- **Aşınma sayacı ve bütçe altyapısı:** Token bucket, birleştirme kuyruğu, hash tekilleştirme. Sonraki fazların hepsi bunun üstüne kurulur.
- **Tak-çıkar ve mod algılama:** `CM_Register_Notification` + hidapi. 2.4G'de ekran kuyruğu.
- **Render çekirdeği:** tiny-skia + cosmic-text + resvg, Türkçe testleriyle. Kartlar UI'da önizlenir.

### Faz 2: Olay tabanlı canlı kartlar + LED canlı katmanı

- **Now Playing:** GSMTC, kapaklı düzen, bekleme süresi, bütçe uyumlu modlar.
- **Pomodoro:** Faz kartları ve LED ilerleme barı.
- **Takvim (ICS):** Sıradaki etkinlik kartı.
- **LED canlı motoru (`04 20`, 30 fps)**
  - CPU/GPU barı, ses düzeyi barı (`IAudioEndpointVolume`).
  - Ses spektrumu (`cpal` loopback).
  - Mikrofon sessiz göstergesi.
  - Caps/Num kilit katmanı.
- **"Canlı mod" anahtarı** ve kapanış davranışı: son görsel ya da saat.

### Faz 3: Gelişmiş

- **Oyun oturumu kartı ve özeti:** Ön plan algılama, NVML/PDH/D3DKMT.
- **Uygulama başına ekran ve LED profilleri:** Ekran için ≥60 s bekleme, ışık yalnız akışla.
- **Hava** (lisans kararından sonra) ve **pano döngüsü** (saatlik, isteğe bağlı).
- **Harici sensör sağlayıcı:** HWiNFO SHM ya da LHM.
- **Bildirim LED'i:** Seyrek paket ve kod imzalama kararından sonra.
- **Şablon sistemi:** Kullanıcının kendi kartını tasarlaması. Veri alanları, yer tutucular ve önizleme.

### Faz 4: Araştırma (yalnızca okuma ve gözlem, cihaza riskli yazım yok)

- **Flash'sız ekran yolu var mı?**
  - Resmi sürücünün güncel sürümleri izlenir (v1.0.0.6 ve sonrası).
  - `SET_TFT_MATERIAL` gibi yeni komut adları takip edilir [S6 board-scan §2.2].
  - 2.4G'deki 28 baytlık `7F 03` yolu ve AK980 firmware güncellemelerinin değişiklik notları izlenir.
- Böyle bir yol bulunursa Faz 2 kartları gerçek zamanlıya taşınır. Mimari zaten "render → hedef" ayrımıyla kurulu olacak.

---

## 7. Kaynaklar

**Cihaz ve topluluk (depolar 2026-11-10'da `--depth 1` ile okundu)**

- **S1:** YellowHearth1/ajazz-ak980-v2-linux, `docs/PROTOCOL.md`, `docs/DESIGN.md`, commit `6963448`. https://github.com/YellowHearth1/ajazz-ak980-v2-linux/blob/6963448/docs/PROTOCOL.md
- **S2:** Aynı depo, `crates/ak980-core/src/lcd/{mod,encode,render}.rs`, `agent/reminders.rs`. https://github.com/YellowHearth1/ajazz-ak980-v2-linux/tree/6963448/crates/ak980-core/src/lcd
- **S3:** donglover69/sonix-keyboard-studio, `README.md`, commit `f66bb72`. https://github.com/donglover69/sonix-keyboard-studio/blob/f66bb72/README.md
- **S4:** Aiacos/ajazz-control-center, `docs/protocols/keyboard/ak980pro_tft_protocol.md`, `ak980pro_vendor.md`, commit `03a29f7`. https://github.com/Aiacos/ajazz-control-center/blob/03a29f7/docs/protocols/keyboard/ak980pro_tft_protocol.md
- **S5:** SlightlyLookAround/ak980_web, `README.md`, commit `703e918`. https://github.com/SlightlyLookAround/ak980_web/blob/703e918/README.md
- **S6:** wsclx/ak820pro-modder, commit `55a2ca2`. Dosyalar: `docs/HANDOFF.md`, `README.md`, `docs/MACOS-800A.md`, `docs/research/2026-10-25-board-scan.md`, `src-tauri/src/now_playing_tft.rs`, `src-tauri/src/tft_memory.rs`. https://github.com/wsclx/ak820pro-modder/blob/55a2ca2/docs/HANDOFF.md
- **S7:** fromagge/ak820-screen, `README.md`, `PROTOCOL.md`, commit `9030b81`. https://github.com/fromagge/ak820-screen/blob/9030b81/PROTOCOL.md
- **S8:** gha-projects/ak820-mac, `docs/pantalla.md`, commit `abd46c4`. https://github.com/gha-projects/ak820-mac/blob/abd46c4/docs/pantalla.md
- **S9:** DarkTiengo/ak820ctl, `README.md`, commit `2903211`. https://github.com/DarkTiengo/ak820ctl/blob/2903211/README.md
- **S10:** fraustiz/freejazz (AK980 Max, ekransız), `crates/freejazz-app/Cargo.toml`, `src/live/audio.rs`, commit `bd2d449`. https://github.com/fraustiz/freejazz
- **S11:** AK980 kullanım kılavuzları (üçüncü taraf kopyalar).
  - DAIDAI AK980: https://manuals.plus/asin/B0DWW7FKBJ
  - MechLands AK980: https://manuals.plus/asin/B0DF7STD5F
- **S12:** Parkablogs AK980 incelemesi: https://www.parkablogs.com/content/ajazz-ak980-wireless-mechanical-keyboard-review
- **S13:** PY25Q128HA, LCSC C18208279: https://www.lcsc.com/product-detail/C18208279.html
- **S14:** Bu proje: `docs/teknik-analiz.md` ([D] donanımda doğrulanan bulgular, 2026-11-10).

**Windows ve servisler**

- **W1:** GlobalSystemMediaTransportControlsSessionManager: https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager
- **W2:** Notification listener: https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener
- **W3:** Paket kimliği ve external location: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/modernize-packaged-apps
- **W4:** Posta ve Takvim desteğinin sonu: https://support.microsoft.com/en-us/office/windows-mail-calendar-and-people-become-outlook-773ecb94-5b16-4155-96e1-bc9afdc08e31
- **W5:** Open-Meteo kullanım koşulları: https://open-meteo.com/en/terms
- **W6:** MET Norway ToS: https://api.met.no/doc/TermsOfService
- **W7:** WinRing0 ve Defender: https://www.neowin.net/news/windows-1110-is-flagging-winring0-on-your-pc-monitoring-fan-control-apps-heres-why/
- **W8:** LibreHardwareMonitor sürümleri: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases/tag/v0.9.5 ve https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases
- **W9:** HWiNFO SHM 12 saat sınırı: https://www.hwinfo.com/forum/threads/shared-memory-12-hour-limit-popup.8326/
- **W10:** D3DKMT_ADAPTER_PERFDATA:
  - https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmthk/ns-d3dkmthk-_d3dkmt_adapter_perfdata
  - https://microsoft.github.io/windows-docs-rs/doc/windows/Wdk/Graphics/Direct3D/struct.D3DKMT_ADAPTER_PERFDATA.html
- **W11:** DXGI_QUERY_VIDEO_MEMORY_INFO: https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/ns-dxgi1_4-dxgi_query_video_memory_info
- **W12:** CM_Register_Notification: https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification
- **W13:** cpal WASAPI loopback ve Microsoft loopback belgesi:
  - https://docs.rs/crate/cpal/0.18.1/source/src/host/wasapi/mod.rs
  - https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording
- **W14:** Görev Yöneticisi CPU formülü:
  - https://www.windowslatest.com/2025/05/26/windows-11-24h2s-task-manager-new-cpu-usage-formula-rolls-out-to-everyone
  - https://support.microsoft.com/en-us/help/3200459
- **W15:** Teams üçüncü taraf API'sinin kaldırılması:
  - https://windowsreport.com/microsoft-teams-drops-elgato-stream-deck-support-leaving-users-frustrated/
  - https://learn.microsoft.com/en-us/answers/questions/5866166/retirement-of-legacy-third-party-meeting-and-call
- **W16:** Discord RPC: https://docs.discord.com/developers/topics/rpc
- **W17:** BLE pil özelliği ve GATT:
  - https://github.com/HyperNylium/Get-Bluetooth-Battery-Persentage
  - https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/gatt-client
  - https://learn.microsoft.com/en-us/archive/blogs/thunbrynt/windows-phone-8-1-for-developersbluetooth-le-battery-service
- **W18:** sysinfo Windows sıcaklık kaynağı: https://github.com/GuillaumeGomez/sysinfo/blob/master/src/windows/component.rs
- **Crate sürümleri:** crates.io API, 2026-11-10.

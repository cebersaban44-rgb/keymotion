# Donanım doğrulama günlüğü — AJAZZ AK980 PRO

Bu dosya yalnızca **bu projede, gerçek cihazda ölçülen** bilgileri içerir. Araştırma kaynaklarından gelen ama henüz ölçülmemiş bilgiler `docs/research/` altında kalır. Her kayıt tarih, ürün yazılımı sürümü ve kullanılan araçla birlikte tutulur.

Cihaz: kullanıcının AK980 PRO'su. Beyaz/lila tuş takımı, ekranlı ve topuzlu sürüm.

---

## 2026-11-10 — Bluetooth modu

Araçlar: WinRT GATT okuması (Python bleak/winrt), `kb list`, `kb battery`.

| Konu | Sonuç |
|---|---|
| BLE adı | `AK980 PRO-1`. Son ek, BT kanalı. |
| PnP ID | `01 45 0C FE FE 38 01`. Kaynak BT SIG, VID 0C45, PID FEFE, sürüm 0x0138. |
| BT modül yazılımı | `2024.07.26 SVN0138` |
| Pil (0x180F) | %94. Rust `kb-transport::ble` ile de okundu. |
| Üreticiye özel HID | **Yok.** Bluetooth üzerinden ayar yapılamaz. |
| 0xFEE0/0xFEE1 | "OTA Channel": WCH OTA. **Asla yazılmaz.** |
| HID raporları | 01 sistem, 02 boot klavye ve LED çıkışı, 03 fare, 05 NKRO, 08 tüketici (consumer), 09 Windows Dial (topuz), 11 Eject ve Apple Fn |

## 2026-11-10 — USB (kablolu) modu

Araçlar: `kb list`, `kb hid-all`, `kb descriptors`, `kb peek`, `kb read-colors`, `kb live`. Ürün yazılımı: USB `bcdDevice` = **0x0106**.

### Fiziksel

- **Mod anahtarı (arka):** soldan sağa **2.4G · USB · BT**. USB orta konumdadır.
  - Anahtar USB konumundan hızlıca geçerse Windows cihazı görür ve 1 saniye içinde kaybeder. Hata: `0xA` / `0xC000000E`.
- **İşletim sistemi anahtarı (arka):** MAC · WIN.
- **Ekran:** Stok ekranda tarih (`2026/11/10`), saat (`08:27:03`), pil/şarj simgesi ve durum simgeleri var. Durum simgeleri: WIN, USB, 2.4G, BT1, kilit, A (Caps), 1 (Num). Sağ tarafta bir görsel bulunuyor.
- **Şarj göstergesi:** Şarj olurken Fn tuşu kırmızı yanıyor.
- **Düzen:** Fotoğraftan ölçüldü (yaklaşık 89.6 px/u). Ayrıntılar `crates/kb-devices/models/ajazz-ak980-pro.toml` içinde.
  - Del, Home, PgUp ve PgDn ayrı bir sütunda.
  - Yön tuşları biraz aşağıda.
  - Ekran ve topuz sağ üstte, numpad'in üzerinde.
  - Sağ Alt tuşu yok.

### USB arayüzleri

| Arayüz | Usage page | Rol |
|---|---|---|
| MI_00 | 0x0001/0x06 | Boot klavye |
| MI_01 | 0x000C/0x01, 0x0001/0x80, 0x0001/0x06, 0x0001/0x02, 0xFFFF/0x01 | Consumer, sistem, NKRO klavye, fare, üreticiye özel. 0xFFFF: rapor 05, 3 baytlık giriş. |
| MI_02 | **0xFF68**/0x61 | Ekran. 64 bayt giriş (onay), **4096 bayt çıkış**. |
| MI_03 | **0xFF13**/0x01 | Komut. 64 bayt giriş, çıkış ve **feature**; rapor kimliği yok. |

- Üretici dizgesi: `SONiX`. Ürün dizgesi: `AK980 PRO` ("SONiX AK980 PRO" değil).

### Komut kanalı (FF13) davranışı

| Ölçüm | Sonuç |
|---|---|
| Pasif GET_FEATURE, hiçbir şey gönderilmeden | 65 bayt, tamamı `00` |
| `04 F5` (renk okuma) | Yankı durumu **01**. 9 paket, 576 bayt. Arka ışık kapalıyken tüm renkler `#000000`. |
| `04 02` (kapanış) yankısı | Durum **00**. Kaynaklarda 01 ve FF diye çelişiyordu. |
| `04 20` (gerçek zamanlı) başlık yankısı | Birebir yankı, durum **00**. 01 değil, yani "ret" anlamına gelmiyor. |
| Yarım bırakılan `04 20` | Başlık gönderilip veri gelmezse klavye sonraki paketleri veri olarak tüketir; sonraki `04 F5` durum 00 ile başarısız olur. Tam bir kare (başlık, 8 veri, `04 02`) oturumu toparlar; ilk kare `FF` alır. |
| `04 20` akışı | 6 sn, **29.5 fps**, 177 kare, 2 ms paket aralığı. Kullanıcı gökkuşağı dalgasını gözle doğruladı. |
| Akış durunca | **Klavye kayıtlı ışık ayarına döner.** Kullanıcı gözlemi: dalga göründü, sonra söndü. |
| `04 F5` ve gerçek zamanlı kare | Akıştan sonra F5 yine siyah döndü. F5 canlı kareyi göstermiyor; teşhis amaçlı. |

### Işık yazma (flash) — 2026-11-10

Araçlar: `kb effect`, `kb per-key`, `kb read-colors`. Tüm komutlar `kb-protocol::sonix::guard` bekçisinden geçti.

| Ölçüm | Sonuç |
|---|---|
| `04 18` (START) | Yankı durumu **01** |
| `04 13 … [8]=01` | Yankı durumu **01** |
| Işık veri paketi | `0B FF 00 00 00 00 00 00 01 05 03 00 00 00 AA 55`: fabrika efekti Rolling, gökkuşağı, parlaklık 5, hız 3 |
| `04 02` yankısı | `04 02 00 01 12 02`. Durum 01; **0x0212 = gönderilen veri baytlarının toplamı** (530). |
| `04 F0` | Yankı okunmadan gönderildi (resmi sürücü gibi). Toplam süre **285 ms**. |
| Sonuç | `F5`, efekt kaydedildikten sonra tuşlarda akan gökkuşağı renklerini döndürdü: Esc #00FDC5, Space #70FD00, Backspace #FA0088, Num+ #0000FD. |
| Tuş başına renk (`80` + `04 23`, 9 paket) | WASD ve oklar #FF8A3D, parlaklık 5. Sağlama toplamı tuttu; süre **856 ms**. `F5` yalnızca bu 8 tuşu #FA883C, kalanını #000000 döndürdü. |

**Sonuçlar:**

- **Sağlama toplamı:** `04 02` cevabındaki toplam, veri bütünlüğü kontrolü olarak kullanılabilir. Sürücü artık uyuşmazlıkta hata veriyor ve `04 F0` göndermiyor.
- **`F5` animasyonları izliyor:** Bu ürün yazılımında `F5` animasyonlu efektlerin o anki karesini de döndürüyor. S2 bunun tersini söylüyordu. Uygulamadaki "Klavyeden canlı okunuyor" aynası bu davranışa dayanıyor (saniyede yaklaşık 4 okuma).
- **Renk hassasiyeti:** Klavye kanal başına yaklaşık 6 bitlik çözünürlükle saklıyor (FF → FA/FD).

### Uygulamada tekrarlanan okuma hatası ve çözümü — 2026-11-10

**Belirti:** Kullanıcı uygulamada üç sorun bildirdi:
- Ayna gerçek ışığı göstermiyordu.
- Canlı önizleme klavyeye ulaşmıyordu.
- "Uygula" klavyeyi değiştirmiyordu.

Komut satırında aynı komutlar çalışıyordu.

| Bulgu | Ölçüm |
|---|---|
| IPC sırası | Tauri çağrıları paralel işliyor. React StrictMode'un "aç → kapat → aç" dizisi arka uca "aç, aç, kapat" sırasıyla ulaştı. **Çözüm:** Durum değiştiren komutlar arayüzde tek bir sıradan geçiyor. |
| GET_FEATURE stall | Bazı okumalar Windows hatası `0x1F` (ERROR_GEN_FAILURE) döndürüyor; yeniden denemede düzeliyor. **Çözüm:** 5, 10 ve 15 ms bekleyerek 3 yeniden deneme. |
| **`04 F5` sayacı** | **Kök neden.** Sayaç `00` iken (S1/S2 biçimi) bir bağlantıdaki yalnızca **ilk** okuma çalışıyor. Sonraki `F5`'lerin yankısı `00`'da kalıyor, oturum kapanmıyor ve ardından gelen komutlar (canlı kare, efekt) yutuluyor. Sayaç **`09`** iken (S4/S8 biçimi) aynı bağlantıda 250 ms arayla 8 okumanın 8'i başarılı ve `04 02` yankısı durum 01 dönüyor. |
| Okuma kapanışı | `04 02`, resmi sürücüdeki gibi 35 ms beklemeyle gönderiliyor. |
| Canlı kare | Başlıktan sonra yankı okunamazsa bile kare (8 veri ve `04 02`) her zaman tamamlanıyor. |

**Düzeltmeden sonra uygulamanın içinden test** (WebView2 uzaktan hata ayıklama ve CDP ile):

- **Ayna:** 6 saniyede 14 okuma, 0 hata. Görsel gerçek renkleri gösteriyor: WASD ve oklar #FA883C, Q #000000.
- **Efekt:** "Nefes" mavi (#3A5BFF) seçilip uygulandı: "Klavyeye kaydedildi · 341 ms". Ayna bütün tuşları mavi-mor (`rgb(78,62,173)`) okudu.
- **Canlı önizleme:** Akış başladı ve gökkuşağı deseni gönderildi; atlanan kare olmadı. Sekmeden çıkınca akış durdu.

### Renk doğruluğu ve gamma — 2026-11-10

**Belirti:** Tuş başına renkte doğru tuşlar yanıyordu, ama paletten seçilen renkler klavyede farklı görünüyordu. Gökkuşağı deseni doğru görünüyordu.

Kullanıcıya gözle yaptırılan testler:

| Test | Sonuç |
|---|---|
| Canlı (`04 20`): Esc kırmızı, F1 yeşil, F2 mavi, F3 beyaz, F4 sarı, F5 camgöbeği, F6 macenta, F7 turuncu #FF8A3D | **Hepsi doğru** |
| Kalıcı tablo (`04 23`): aynı desen | **Hepsi doğru**. Kanal sırası iki yolda da R, G, B. |
| Uygulamanın gönderdiği kare | Doğru (`1=#FF3B30`, `2=#34C759`). Hata arayüzde değil. |
| Palet renkleri: 1–6 ham, Q–Y gamma 2.2 | **Q–Y paletteki renklere daha yakın** |

**Neden:** Monitör renkleri sRGB gamma eğrisiyle gösterir; LED PWM ise doğrusaldır. Ara değerler (örneğin #8B5CF6'daki 0x5C) klavyede çok parlak yanıyor ve renk soluk, beyazımsı çıkıyor. Saf renkler (kanal başına 0 ya da 255) gamma'dan etkilenmediği için ilk testler bu hatayı yakalayamadı.

**Çözüm:** `kb-core::color::LedCalibration`. Varsayılan gamma 2.2; kanal başına kazanç ayarlanabilir. Cihaz aktörü canlı karelere ve tuş başına tablolara bu dönüşümü uyguluyor. Firmware efektlerinin rengine (`04 13`) henüz uygulanmıyor; ayrıca doğrulanacak.

### Kalibrasyon arayüzü, uçtan uca — 2026-11-10

CDP ile gerçek uygulamada, klavye USB modundayken denendi:

- **Test deseni (gri basamaklar):** 97 LED'lik canlı akış başladı. Gamma kaydırıcısı 1.6 → 1.8 → 2.2 oynatılınca aktör her değeri aldı ve bir sonraki kareyi yeni değerle gönderdi.
- **Kalıcılık:** Kaydırıcı 400 ms durunca `%APPDATA%\dev.klavye.studio\calibration.json` yazıldı. "Varsayılana dön" 2.2'yi geri yazdı.
- **Akışın bitişi:** Desen kapatılınca akış durdu ve klavye kayıtlı ışığına döndü.
- **Ayna:** `04 F5` okumaları ters kalibrasyonla gösteriliyor. Tuş başına modda boyanan renkler aynen geri okunur. Firmware efektlerinde düşük PWM değerleri ekranda daha açık görünür; LED'in gerçek görünüşü de budur.
- **Gözlem:** Yaklaşık 60 saniyelik akışta (yaklaşık 1200 kare) bir `04 20` karesi `FF` ile reddedildi. Sonraki kare 50 ms sonra sorunsuz gitti; aktör hatayı log'layıp devam ediyor. Sıklık artarsa incelenecek.
- **Göz testi:** Kullanıcı gri basamakları ve beyazı gözle denedi; ikisi de doğru.

### Eksik tuşlar bir önceki rengini koruyor — 2026-11-10

**Belirti (kullanıcı):** Tuş başına renkte canlı önizleme açıkken:
- "Temizle" ekrandaki tuşları söndürdü ama klavyedeki renkler kaldı.
- Tıklanan tuşlar seçilen rengi almıyor gibi görünüyordu.
- Eski renkler klavyede kalmaya devam etti.

**Ölçüm (`kb live-probe`, yalnızca RAM; `04 F5` canlı katmanı okuyabiliyor):**

| Aşama | Gönderilen kare | `F5` sonucu |
|---|---|---|
| A | 97 tuşun hepsi mavi | 95 tuş mavi |
| B | Yalnızca Esc kırmızı, diğer yuvalar `00 00 00 00` | Esc kırmızı, **94 tuş hâlâ mavi** |
| C | Esc kırmızı, diğer tuşlar `[idx 0 0 0]` | 94 tuş siyah |
| B (düzeltmeden sonra) | Yalnızca Esc kırmızı | 94 tuş siyah |

- **Neden:** Firmware karedeki girişleri **idx baytına göre** uyguluyor. Tamamı sıfır olan yuva yalnızca "LED 0" anlamına geliyor ve hiçbir tuşu değiştirmiyor. Uygulama yalnızca boyanmış tuşları gönderiyordu. Bu yüzden boyanmayan tuşlar önceki karedeki renklerini korudu; buna kalibrasyon test deseni de dahil. Boş tasarım ise hiçbir şeyi söndürmedi.
- **Neden testlerde görülmedi:** CLI desenleri her zaman 97 tuşun hepsini, siyahları da açıkça yazarak gönderiyordu.
- **Çözüm:** `realtime_table` ve `per_key_table` her yuvaya kendi idx'ini yazıyor; verilmeyen LED'ler siyah gidiyor.

**Uygulamada kayıt (CDP, flash):**
- 97 tuş beyaz kaydedildi (869 ms). Sağlama toplamı 16 biti aştığı halde tuttu; firmware toplamı 16 bitte sarıyor. Ayna 96 tuşu beyaz okudu.
- Ardından yalnızca Esc kırmızı kaydedildi. Ayna 94 tuşu siyah, Esc'i #FE3B30 okudu; ters kalibrasyon boyanan rengi geri verdi.
- Testten sonra kullanıcının önceki efekti (15, kırmızı) yeniden uygulandı.

**Göstergeler:**
- Num Lock (LED 32, beyaz) ve Fn (LED 96, kırmızı) ne canlı kareye ne de kayıtlı tabloya uyuyor; firmware bunları kendisi yakıyor.
- Num Lock'un beyazı muhtemelen Num Lock'un açık olduğunu gösteriyor. Fn'in kırmızısının neyi gösterdiği henüz bilinmiyor.

**Arayüz hatası (ayrıca):** Fırça rengi değiştikten hemen sonra tıklanan tuş, React yeniden çizmeden önce eski rengi alıyordu. Boyama artık rengi tıklama anında durum deposundan okuyor.

### Ekran: stok ana sayfa (fotoğraf, 2026-11-10 08:29)

- **Yerleşim:** Panel yatay ve topuzun hemen solunda. Panelin sol yarısı mavi bir durum kartı, sağ yarısı siyah zemin üzerinde çizgi çizimi dağlar ve kuşlar.
- **Durum kartının içeriği:**
  - Sol üstte tarih (`2026/11/10`).
  - Sağ üstte şimşek simgesi ve pil (şarj oluyor).
  - Ortada **saniyeli** saat (`08: 29: 41`), düz serif rakamlarla.
- **Durum simgeleri (soldan sağa):**

  | Simge | Renk | Durum |
  |---|---|---|
  | WIN | camgöbeği | dolu |
  | USB (∪) | mor | dolu, yani etkin bağlantı |
  | 2.4G alıcı | yeşil | çerçeve |
  | BT1 | mavi | çerçeve |
  | Win kilidi (asma kilit) | sarı | çerçeve |
  | A (Caps) | sarı | çerçeve |
  | 1 (Num) | sarı | çerçeve |

- **Görünen en-boy oranı:** Yaklaşık 2.8:1. Fotoğraf açılı çekildiği için dikey boyut kısa görünüyor olabilir.
- **[AÇIK SORU]:** Yükleme formatı 240×135 (resmi sürücünün yapılandırma dosyası; S1 ve S2 bu klavyede doğrulamış). Bu görüntünün nerede gösterildiği bilinmiyor. İki olasılık var:
  - Ana sayfanın sağ panelinde.
  - Fn+Home ile açılan, tam ekran animasyon sayfasında.

  İlk ekran yüklemesinde bir test deseniyle (köşe işaretleri ve ızgara) netleştirilecek.
- **Uygulamadaki taklit:** `KeyboardView.tsx` içindeki `MiniScreen` bu fotoğrafa göre çizildi. Etkin bağlantı simgesi, uygulamanın algıladığı moda göre doluyor.

### Sonuçlar

1. **Durum baytı kuralı:** Yalnızca `FF` ret demektir. `00` ve `01` kabul olarak sayılır.
2. **Atomik kare:** Bir `04 20` başlığı gönderildiyse kare her koşulda tamamlanmalıdır. Sürücü artık bunu yapıyor.
3. **Canlı efektler:** Uygulama çalıştığı sürece sürekli akış gerekir. Uygulama kapanınca klavye güvenle kendi ayarına döner.

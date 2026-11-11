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
- ~~**Renk hassasiyeti:** Klavye kanal başına yaklaşık 6 bitlik çözünürlükle saklıyor (FF → FA/FD).~~ **Yanlış yorum** (düzeltme 2026-11-10, aşağıda "Efekt rengi, parlaklık ölçeği ve F5'in kırmızı baytı"): FD, 5. parlaklık seviyesinin ölçeği (`255·254 >> 8`). FA ise `F5`'in kırmızıyı `2·r mod 256` diye bildirmesinden geliyor (`2·253 mod 256 = 250`). Renkler 8 bit saklanıyor.

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

**Çözüm:** `kb-core::color::LedCalibration`. Varsayılan gamma 2.2; kanal başına kazanç ayarlanabilir. Cihaz aktörü canlı karelere, tuş başına tablolara ve (2026-11-10'dan beri) firmware efektlerinin rengine (`04 13`) bu dönüşümü uyguluyor. Firmware efekt rengini kendisi gamma'lamıyor; ölçüm aşağıda.

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

### Efekt rengi, parlaklık ölçeği ve F5'in kırmızı baytı — 2026-11-10

**Soru:** Firmware efekt rengine (`04 13`) kendisi gamma uyguluyor mu? Uygulamanın kalibrasyonu efekt rengine de uygulanmalı mı?

**Yöntem:** Uygulama kapalıyken CLI ile Static efekti (mod 1) farklı renk ve parlaklıklarda kaydedildi. Her seferinde `04 F5` ile geri okundu. Statik efektte bütün tuşlar aynı değeri döndürüyor. Testten sonra kullanıcının kayıtlı durumu (tuş başına mod, bütün tuşlar siyah, parlaklık 5) geri yazıldı ve `F5` ile eşleştiği görüldü.

| Kaydedilen (Static) | `F5` (R G B) | Yorum |
|---|---|---|
| #8B5CF6, parlaklık 5 | `12 5B F4` | G ve B: `v·254 >> 8` (5C→5B, F6→F4). Gamma yok. |
| #404040, 5 | `7E 3F 3F` | R = 2·63 |
| #808080, 5 | `FE 7F 7F` | R = 2·127 |
| #FFFFFF, 1 / 2 / 3 / 4 / 5 | `5E 2F 2F` / `BE 5F 5F` / `1C 8E 8E` / `7C BE BE` / `FA FD FD` | G = B = 47, 95, 142, 190, 253. R = 2·G mod 256. |

**Canlı kare (`kb live-probe`, D ve E aşamaları, yalnızca RAM):** #808080 → `00 80 80`; #8B5CF6 → `16 5C F6`; #FF0000 → `FE 00 00`. Canlı kareye parlaklık ölçeği uygulanmıyor; kırmızı yine `2·r mod 256`. Kullanıcı canlı gri basamakları daha önce gözle gri görmüştü (§ Kalibrasyon arayüzü). Demek ki ışığın kendisi doğru; tuhaflık yalnızca okumada.

**Sonuçlar:**
1. **Efekt rengi doğrusal:** Firmware efekt rengini gamma'lamıyor; tuş başına tablo ve canlı kareyle aynı biçimde sürüyor. Uygulama artık kullanıcının kalibrasyonunu efekt rengine de uyguluyor. Böylece aynı palet rengi her modda aynı görünüyor.
2. **Parlaklık ölçeği:** Seviye `n`'de değer `v·m[n] >> 10`, `m = [0, 191, 382, 573, 764, 1016]`. Yani 1–4. seviyeler `191·n / 1024`, 5. seviye `1016 / 1024`. Doğrusala yakın ama `n/5` değil (3. seviye %56, %60 değil). Canlı önizleme artık bu tabloyu kullanıyor. Tuş başına tablo da aynı ölçeği kullanıyor (aşağıdaki ölçüm).
3. **`F5` kırmızıyı `2·r mod 256` bildiriyor:** Kırmızının üst biti kayboluyor; her bayt iki değere karşılık geliyor (`b/2` ya da `b/2 + 128`). Yeşil ve mavi olduğu gibi geliyor. Göstergeler de aynı: Num Lock `FE FF FF` (beyaz), Fn `FE 00 00` (kırmızı).
   - **Etkisi:** Ayna kırmızı ara tonları yanlış gösteriyordu. Örneğin mor #8B5CF6 lacivert (#125BF4) görünüyordu. Daha önceki "Nefes mavi #3A5BFF → ayna mor" gözlemi de bu.
   - **Çözüm:** `kb_protocol::sonix::readback_red_candidates` iki adayı veriyor. `kb_core::readback` uygulamanın beklediği renge göre birini seçiyor. Beklenen renk, kayıtlı tasarım, efekt rengi ya da modeldeki gösterge rengi olabilir. Beklenti yoksa (gökkuşağı efektleri) karedeki en parlak kanala göre tahmin ediyor. Gökkuşağında yeşil ya da mavi zaten tepedeyken kırmızı belirsiz kalıyor; bu bilinen bir sınır.
   - **Uygulamada doğrulandı (CDP):** Static #8B5CF6 uygulandı. Ayna tuşları `#895BF4` (137, 91, 244) gösterdi, göstergeleri `#FFFFFF` ve `#FF0000`. Sonra kullanıcının durumu geri yüklendi.

**Tuş başına tablonun parlaklık ölçeği** (kullanıcının isteğiyle, CLI ile, aynı gün): Altı tuş 5'ten 1'e her seviyede kaydedildi ve `F5` ile okundu. Ardından kullanıcının tasarımı (hepsi siyah, parlaklık 5) geri yazıldı ve `F5` ile eşleşti.

| Kaydedilen renk | 1. seviye | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|
| #FFFFFF | 47 | 95 | 142 | 190 | 253 |
| #808080 | 23 | 47 | 71 | 95 | 127 |
| #8B5CF6 (R, G, B) | 25, 17, 45 | 51, 34, 91 | 77, 51, 137 | 103, 68, 183 | 137, 91, 244 |
| #404040 | 11 | 23 | 35 | 47 | 63 |
| #10C0FF (R, G) | 2, 35 | 5, 71 | 8, 107 | 11, 143 | 15, 190 |

Kırmızı değerler `2·r mod 256` baytından çözüldü. 40 değerin hepsi `v·m[n] >> 10` tablosuna birebir uyuyor; efektlerdeki beyaz ölçümleri de aynı. İlk tahmindeki `m/256` tablosu (`48, 96, …`) 1. ve 2. seviyede bir birim şaşıyordu (örneğin #808080 2. seviyede 47 okunuyor, 48 değil).

### Saat senkronu (`04 28`) — 2026-11-10

**Akış** (resmi sürücüyle aynı, `04 F0` yok): `04 18` → `04 28 … [8]=01` → 1 veri paketi → `04 02`. Veri: `00 01 5A YY MM DD hh mm ss 00 DOW`, ikili (BCD değil), yerel saat, son iki bayt `AA 55`.

| Adım | Ölçüm (`kb clock`, trace) |
|---|---|
| `04 18`, `04 28` | Yankılar durum **01** |
| Veri `00 01 5A 1A 0B 0A 05 09 00 00 02 …` (2026-11-10 05:09:00, Salı) | Klavye veri paketini **bayt bayt aynen** geri yansıttı |
| `04 02` | Durum **01**, ama bayt toplamı **`00 00`**. Işık akışlarında bu alan veri toplamını veriyordu; saat akışında vermiyor. |
| Süre | 251 ms |

**Sonuç:** Saat akışında teslimatın kanıtı toplam değil, **veri yankısı**. Sürücü yankının gönderilen paketle birebir aynı olmasını şart koşuyor; `04 02`'de yalnızca ret (`FF`) olmadığına bakıyor.

**Uygulamada (CDP):**
- `clock.json` yokken uygulama açıldı. Klavye bağlandıktan 2,3 sn sonra saat kendiliğinden ayarlandı ve `clock.json` yazıldı.
- Genel Bakış → "Klavye saati" → "Şimdi eşitle": 250 ms, "Saat eşitlendi". "Bağlanınca eşitle" anahtarı dosyaya kaydedildi.
- **Gözle doğrulama:** Kullanıcı klavyenin ekranındaki saatin bilgisayarla aynı olduğunu onayladı (2026-11-10).

### Efekt yönleri — 2026-11-10

**Soru:** Yön kodları (ışık paketinin 11. baytı) hangi efektte ne yapıyor? Kaynaklar çelişiyordu (0 = sol, 1 = aşağı, 2 = yukarı, 3 = sağ; ya da yalnızca 0/1; mod 10 için 4 yön).

**Yöntem (`kb direction-probe`):**
- Her yönlü efekt gökkuşağı modunda her yön koduyla kaydedildi (flash, toplam yaklaşık 30 yazım).
- 1,2 sn sonra `04 F5` ile 6–10 kare art arda okundu.
- Desenin kayması iki yolla hesaplandı:
  - Kareler x, y ve merkeze göre açı eksenlerine izdüşürüldü.
  - Ayrıca iki boyutlu bir eşleştirme yapıldı.
- Kırmızı belirsiz okunduğu için yalnızca yeşil ve mavi kullanıldı.
- Akış (Flowing) için yanan LED indeksleri kare kare listelendi.
- Sonunda kullanıcının efekti (Launch, #8B5CF6, parlaklık 5, hız 3) geri yazıldı.

| Mod | Kod 0 | Kod 1 | Kod 2 | Kod 3 | Ölçüm |
|---|---|---|---|---|---|
| 10 Scrolling | ↓ aşağı | ↑ yukarı | ↑ yukarı | ↓ aşağı | y ekseni: hız 0–1'de ±0,4, hız 4'te ±1,2 tuş/sn; x'te hareket yok |
| 11 Rolling | → sağa | ← sola | ← sola | → sağa | x ekseni: ±5,7 tuş/sn (hız 1), güven 2,3–3,2 |
| 12 Rotating | ↺ saatin tersi | ↻ saat yönü | ↻ saat yönü | ↺ saatin tersi | açı ekseni: −16,5 / +25 °/sn |
| 16 Flowing | ileri | geri | geri | ileri | 6 LED'lik bir katar **LED indeks sırasıyla** ilerliyor: kod 0'da indeks artıyor (22–27 → 26–31 → …), kod 1'de azalıyor. Ekranda satır satır, soldan sağa ve yukarıdan aşağı. |
| 18 Tilt | ↗ sağ yukarı | ↙ sol aşağı | ↙ sol aşağı | ↗ sağ yukarı | iki boyutlu eşleştirme: (+4, −2) / (−2, +4) tuş/sn |

**Sonuçlar:**
- Her yönlü efektin **iki** yönü var. Kod 2, 1 gibi; kod 3, 0 gibi davranıyor. Satıcı tablosunda mod 10 dört yönlü (`caps` 0x3B) görünüyordu; bu firmware'de öyle değil.
- Uygulama her efekt için iki seçenek gösteriyor ve bunları hareketin kendisiyle adlandırıyor (Aşağı/Yukarı, Sağa/Sola, Saat yönü/Saatin tersi, İleri/Geri, Sağ yukarı/Sol aşağı). Katalog bilgisi: `kb_protocol::sonix::Motion`.
- **Kullanıcı denemesi:** Kullanıcı yön düğmelerini uygulamada seçip klavyede gördü: yazdığı gibi çalışıyor.
- **Yan gözlem:** Akış efekti geri yönde katarın başına (indeks ~0–20) geldiğinde, `F5` modelde tuşu olmayan yaklaşık 45 LED indeksini de yanık döndürdü. İndeksler: 0, 14–18, 35, 36, 53, 54, 71, 72, 89, 90, 95, 97, 107–109, 112–116, 120, 124–143. Bunlar yan ışık şeridi ya da firmware'in boş indeksleri olabilir (açık soru, DURUM.md §8).

### Tuş atama (`04 11` / `04 27`) — 2026-11-10

**Soru:** Kaynaklardaki keymap biçimi (§6) bu klavyede çalışıyor mu? Hangi girişler ne gönderiyor, varsayılana dönüş nasıl?

**Önce:** Kullanıcı atamayı hiç değiştirmediğini söyledi (fabrika ayarı). Klavye keymap'i geri okuyamadığı için test sonunda iki katman da sıfır tabloyla yazıldı.

**Yöntem 1 (CLI, `kb keymap`):**
- Temel katman: Num / → `02 00 04 00` (A), Num * → `03 E9 00 00` (Ses+), Num − → `02 02 04 00` (Shift+A), Num + → `01 01 08 00` (fare `08`).
- Fn katmanı: Num / → `02 00 05 00` (B).
- Her yazım: `04 18` → `04 11`/`04 27` (`[8]=09`) → 9 veri → `04 02` → `04 F0`. 608 ms. `04 02` yankısındaki toplam gönderilen 576 baytla aynı; `04 F0` yankısı da geldi (durum `01`).
- Kullanıcı tuşlara tarayıcıdaki bir test sayfasında bastı. Sayfa `keydown`/`keyup`, fare ve tekerlek olaylarını zaman damgasıyla kaydetti.

| Tuş | Gelen olay | Sonuç |
|---|---|---|
| Num / | `KeyA`, `"a"` | ✅ |
| Num * | `key="AudioVolumeUp"`; Windows ses göstergesi açıldı | ✅ medya (consumer) |
| Num − | `ShiftLeft` basıldı, sonra `KeyA` `"A"` (shift) | ✅ değiştirici + tuş |
| Num + | `mousedown button=3` (tarayıcıda "geri") | ✅ `08` = 4. düğme, yani **geri** |
| Fn + Num / | `KeyB`, `"b"` | ✅ Fn katmanı |

- Sıfır tabloyla yazdıktan sonra Num / yine `NumpadDivide "/"`, Num * yine `NumpadMultiply "*"` gönderdi ve ses açılmadı: **varsayılana dönüş = bütün girişler sıfır** doğrulandı.

**Yöntem 2 (uygulama):** Aynı atamalar (Num + bu kez "Devre dışı", `05 03 00 00`) Tuş Atama sayfasının "Klavyeye kaydet" düğmesiyle yazıldı (631 ms). Kullanıcı sayfadaki tuş testi kutusunda bastı: A, Sesi aç, Shift + A geldi; Num + hiçbir şey göndermedi. Sonra "Bu katmanı fabrika ayarına döndür" ile kaydedildi. `keymap.json`'da temel katman boş, son yazım zamanı işli.

**Sonuçlar:**
- §6.2'deki giriş kodları bu klavyede geçerli: `02` tuş + değiştirici, `03` consumer, `01 01` fare, `05 03` devre dışı, sıfır = varsayılan.
- Fare yan tuşları: satıcı tablosu (S1 KeyMap.cs) `10` için "tuş 4", `08` için "tuş 5" diyor. Ölçümde `08`, Windows'ta 4. düğme (geri) olarak geldi; yani HID bit sırası geçerli: `08` geri, `10` ileri. `10` ayrıca denenmedi.
- Fn katmanı, kendi Fn işlevi olmayan bir tuşta (Num /) çalışıyor.
- **Ders:** Talimattaki "Num /" yazısı "Num Lock + /" diye okundu ve ilk denemede her basıştan önce bir `NumLock` göründü. Zaman damgaları, Num Lock'un test tuşuyla birlikte basılı tutulduğunu gösterdi; ikinci denemede tuşlar ayrı ayrı doğrulandı.

### Makrolar (`04 19` → `04 15`) — 2026-11-10

**Soru:** Kaynaklardaki makro görüntüsü (§7) ve akış bu klavyede çalışıyor mu? `04 19` yankı veriyor mu, `04 02` toplamı tutuyor mu, beklemeler uygulanıyor mu?

**Önce:** Kullanıcı daha önce makro kaydetmediğini söyledi; `04 19` bütün makroları siler.

**Yöntem (`kb macros`, `kb keymap`):**
- Makrolar:
  - **Yuva 0:** "ok" (o ve k, bas-bırak, her adımdan sonra 20 ms).
  - **Yuva 1:** boş (`FF FF FF FF`).
  - **Yuva 2:** sağ tık bas-bırak, 200 ms, x bas-bırak.
- Görüntü 9 sayfa, bellek 88/2700 bayt. Akış `04 19` → `04 15 [8]=09` → 30 ms → 9 sayfa → 30 ms → `04 02`, 600 ms.
- Temel katman bağlamaları:
  - Num / → `06 00 00 00` (yuva 0, bir kez).
  - Num * → `06 00 01 03` (yuva 0, 3 kez).
  - Num − → `06 02 00 00` (yuva 2).
- Kullanıcı tuşlara zaman damgalı test sayfasında bastı.

| Tuş | Gelen olaylar | Sonuç |
|---|---|---|
| Num / | o↓ o↑ k↓ k↑, adımlar arası ~20 ms | ✅ |
| Num * | "ok" üç kez art arda (tekrarlar arası ~10 ms) | ✅ N kez |
| Num − | sağ tık ↓↑ (20 ms), 218 ms sonra x↓ x↑ | ✅ fare ve bekleme |

**Sonuçlar:**
- `04 19` yankı veriyor (durum `FF` değil). Öncesinde `04 18` gerekmiyor (açık soru §14.10 kapandı).
- `04 02` yankısındaki bayt toplamı, gönderilen görüntünün toplamıyla aynı. Saat akışındaki gibi 0 dönmüyor, yani teslimat toplamla doğrulanabiliyor.
- Görüntü biçimi doğru:
  - Dizin ofsetleri ve boş yuva (`FF FF FF FF`) çalışıyor.
  - Olay sayacı her adım için 2.
  - Bekleme `MS_LO MS_HI 00 50`.
  - Sayfa sayısı stok formülüyle.
- `06 SLOT MODE COUNT`: mod 0 bir kez, mod 1 COUNT kez oynatıyor. Mod 2 ve 3 (tekrar basana kadar, basılı tutuldukça) denenmedi.
- Test sonunda `kb keymap --layer base` ve `kb macros` (boş) yazıldı: atama varsayılan, makro yok.

### Makrolar sayfası, uçtan uca — 2026-11-10

- **Kayıt (CDP):** Makrolar sayfasında yeni makro açıldı, "Kayda başla"dan sonra tuş olayları gönderildi: Shift+H, `Semicolon` (basılı, tekrarlı), I.
  - Adımlar konuma göre HID koduna çevrildi (`Semicolon` → 0x33).
  - Tekrarlar yok sayıldı; kayıt durunca basılı kalan tuş bırakıldı.
  - Gerçek bekleme süreleri yazıldı. "+ Sol tık" bas-bırak çifti ekledi.
- **Etiketler:** WebView2'de `navigator.keyboard.getLayoutMap()` çalışıyor (güvenli bağlam). Kullanıcının düzeninde `Semicolon` = ş, `KeyI` = ı, `Quote` = i, `BracketLeft` = ğ, `Comma` = ö. Adım listesinde 0x33 "Ş" olarak görünüyor.
- **Klavye:**
  - Uygulamadan makro (Shift+H, Ş, I, sol tık) ve atama (Num / → makro 1, 3 kez) kaydedildi; iki yazım da sağlama toplamıyla doğrulandı.
  - Kullanıcı Tuş Atama'daki tuş testi kutusunda Num /'a bastı: "Shift + H, Ş, I, Fare Sol tık" dizisi tekrar tekrar geldi. Kutu o sırada son 10 olayı tuttuğu için iki tur görüldü; sınır sonra 24'e çıkarıldı.
- **Silme:** Makro silinirken bağlı tuş (`/`) uyarıda listelendi ve atama taslağından kaldırıldı. Makrolar ve atama uygulamadan kaydedildi: `macros.json` boş, temel katman boş; klavye fabrika durumunda.

### Ekran: stok ana sayfa (fotoğraf, 2026-11-10 08:29)

- **Yerleşim:** Panel yatay ve topuzun hemen solunda. Işıklı alan mavi bir durum kartı; yanındaki siyah zemin üzerinde çizgi çizimi dağlar ve kuşlar var.
  - **Düzeltme (ikinci fotoğraf ve kullanıcının açıklaması, 2026-11-10):** Dağlar ekranın parçası değil; ekranın çevresindeki siyah çerçeveye basılmış süs. Ekranın kendisi yalnızca ışıklı dikdörtgen: yaklaşık 25×15 mm, yani 1,14" 240×135 panelin tamamı. Durum kartı da ana sayfanın tamamı.
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

- **Görünen en-boy oranı:** Yaklaşık 2.8:1 ölçülmüştü; bu, çerçevedeki dağlar dahil bütün şeridin oranı.
- **[ÇÖZÜLDÜ, E1]:** Yüklenen 240×135 görüntü ana sayfada değil, Fn+Home ile açılan ayrı, tam ekran GIF sayfasında gösteriliyor (aşağıda "Ekran yüklemesi").
- **Uygulamadaki taklit:** `KeyboardView.tsx` içindeki `MiniScreen` bu fotoğrafa göre çizildi; çerçevedeki dağları da çiziyor. Etkin bağlantı simgesi, uygulamanın algıladığı moda göre doluyor. Ekran Stüdyosu yapılırken ekran alanı ile çerçeve ayrımı gözden geçirilecek.
- **Fn tuşunun kırmızısı:** Şarj göstergesi. Kullanıcı 2026-11-10'da gözledi: klavye şarj olurken Fn kırmızı yanıyor (ışık kapalı modda bile).

### Ekran yüklemesi (`04 72` + FF68) — 2026-11-10

Yöntem: `kb screen-test --yes` (E1) ve `kb screen-gif <dosya> --yes` (E4); uygulama kapalıyken, kablolu, fw 0x0106. Ayrıntı: `research/ekran-derin-arastirma.md`.

- **Fabrika animasyonunun yedeği:**
  - Kaynak: AJAZZ'ın sitesindeki "AJAZZ_AK980 V2_Tri-mode Screen version_RGB_Keyboard driver(986086…).rar" (5.146.495 bayt, SHA-256 `50b5338f…6ea44`). Kullanıcı onayıyla indirildi; hiçbir şey çalıştırılmadı.
  - Arşiv Windows'un `archiveint.dll`'i ile açıldı. İçindeki Inno Setup 6.1 kurulum dosyasının veri bloğu Python `lzma` ile çözüldü.
  - Paketteki yapılandırma bu klavyeyi tanımlıyor: `product_name="AK980 PRO"`, `pid="8009"`; 2.4G `"AJAZZ AK980 PRO 2.4G"`.
  - `0.gif`: 240×135, 135 kare, 238.733 bayt, SHA-256 `4d245d45…598323b`. AJAZZ logo animasyonu.
  - Yedek: `%APPDATA%\dev.klavye.studio\screen\factory\ak980-pro-factory.gif` (+ paketin iki XML'i).
- **Kullanıcının karşılaştırması:**
  - Fn+Home GIF sayfasını açtı; aynı AJAZZ logo animasyonu oynuyordu.
  - Hızı, tarayıcı önizlemesiyle (kare başına 100 ms) aynıydı. GIF'teki gecikme 0 olduğu için tarayıcının 100 ms kuralı kullanılıyor.
- **E1, köşe işaretli tek kare:**
  - 16 sayfa, toplam 2,7 s.
  - Sayfa süresi: ilk 283 ms, sonrakiler 141–145 ms, son 184 ms.
  - `04 72` yankı durumu `01`.
  - Her sayfanın onayı Input raporu `01 5A 02 00 …`.
  - `04 02` yankısı `04 02 00 01 00 00 00 00`: durum 01, **toplam 0**, saat akışındaki gibi. Ekran yüklemesinde aktarım bu toplamla doğrulanamıyor; güvence her sayfanın onayı.
  - Kullanıcı gözlemi:
    - Desen doğru yönde: sol üst kırmızı, sağ üst yeşil, sol alt mavi, sağ alt sarı; ok yukarıda.
    - Bantlar kırmızı-yeşil-mavi-gri sırasında; RGB565 LE doğru.
    - Fn+Home saat sayfası ile desen arasında geçiyor; saat sayfası sağlam.
- **E4, fabrika animasyonunun geri yüklenmesi:**
  - 135 kare, kare başına 100 ms (bayt 50), 2136 sayfa.
  - Toplam 308 s, sayfa başına ortalama 143 ms (en kısa 133, en uzun 283). Hata yok.
  - Kullanıcı gözlemi:
    - Yükleme boyunca ekranda LOADING ve yüzde göründü.
    - Sonra Fn+Home ile açılan animasyon fabrikadakiyle görüntü ve hız olarak aynı.
    - Işık kapalı modda olduğu için yükleme sırasında ışık davranışı görülemedi. Yazmanın sürüp sürmediği gözlenmedi.
- **Sonuç:**
  - Sayfa süresi ~143 ms, flash yazma süresiyle uyumlu. Yükleme RAM'e değil flash'a gidiyor (LT-104'ün "RAM tamponu" iddiası bu klavye için geçersiz).
  - Yükleme yalnızca GIF sayfasını değiştiriyor; ana sayfa etkilenmiyor.
  - Bu güne kadar yükleme sayısı: **2** (E1, E4).
- **Açık kalanlar:**
  - E3: panelin gerçek kare hızı.
  - Yükleme sırasında ışıkların davranışı (kullanıcının ışığı kapalı).

### Ekran Stüdyosu, uçtan uca — 2026-11-10

Uygulamanın Ekran sayfası; kullanıcı kendi dosyalarıyla denedi.

- **Kütüphane:** Fabrika yedeği uygulama açılınca kütüphaneye kendiliğinden eklendi. Arayüzden içe aktarılan aynı GIF, Rust'ın çevirdiğiyle bayt bayt aynı çıktı ve ikinci kopya oluşmadı (CDP).
- **Kullanıcının denemesi:**
  - Kendi resmini ve GIF'ini uygulamadan yükledi; önizleme klavyedekiyle aynı.
  - **Yükleme sırasında klavye yazmaya devam ediyor** (Not Defteri'nde denendi).
  - Fn+Home GIF sayfasında yüklenen görseli gösteriyor; ana sayfaya da geçiyor.
  - **E2, kalıcılık:** USB kablosunu çıkarıp taktı, 2.4G moduna geçip döndü; görsel GIF sayfasında kaldı. N1'in "fişten sonra kayboluyor" bildirimi bu klavyede geçerli değil.
  - Uygulama kaydında uygulamadan yapılan 2 yükleme var. Toplam yükleme sayısı: **4** (E1, E4 ve kullanıcının 2 yüklemesi).
- **Kullanıcının akış geri bildirimi** (aynı gün düzeltildi, aşağıda): Yükleme bittikten sonra alan düzenleme ekranında kalıyordu. "Başka dosya" düğmesi silik ve anlamsızdı. "Kütüphaneye kaydet" ile "Klavyeye yükle"nin farkı belli değildi ve Vazgeç düğmesi yoktu.
- **Yeni akış:**
  - Dört adım: Dosya → Düzenle → Önizle → Yükle.
  - Her adımda görünür bir "Vazgeç" var.
  - Önizleme adımında ana düğme "Klavyeye yükle", yanında "Düzenlemeye dön". "Yüklemeden kaydet" ikincil; açıklaması "klavye bağlı değilken ya da sonra yüklemek için". Yükleme zaten kütüphaneye de kaydediyor.
  - Yükleme sırasında ilerleme stüdyonun içinde gösteriliyor. Bitince alan başlangıç ekranına dönüyor ve "… klavyeye yüklendi · Fn+Home" bilgisini veriyor.
  - Onay penceresi Esc ile kapanıyor; açılınca odak Vazgeç'te.
  - Tarayıcıda sahte arka uçla (`?mock=usb`) bütün yollar denendi: yükleme, yüklemeden kaydetme, düzenlemeye dönme, Vazgeç, Esc ve kütüphaneden yükleme.

### Ekran Faz B ve C, klavyede — 2026-11-10

- **Faz B (kullanıcı):** Yazı kartları, slayt ve "Işıkları uydur" klavyede denendi.
  - Yükleme tarafı sorunsuz.
  - "Işıkları uydur"da tuşlar yandı ama renkler görsele benzemedi (aşağıda).
- **Canlı kare kayıtlı ışık "Kapalı" iken (`kb live-probe`, yalnızca RAM, uygulama kapalıyken):**
  - Klavyenin kayıtlı ışığı mod 0 (Kapalı) iken canlı kareler kabul edildi ve `F5` ile okundu:
    - A: 97 tuş mavi;
    - B ve C: yalnızca Esc kırmızı, diğerleri siyah;
    - D: gri `#008080` (kırmızı `2·r mod 256`);
    - E: mor `#165CF6`.
  - Yani canlı önizleme ışık kapalıyken de çalışıyor. Uygulama günlüğündeki birkaç `04 20` reddi (11 sn'de 3) bilinen ara sıra `FF`.
- **"Işıkları uydur" renkleri:**
  - Mor "Günaydın" kartının (Neon) ilk paleti mor + beyaz + pembeydi; beyaz yazı palete girmişti.
  - Düzeltmeden sonra palet mor `#791BFF`, pembe `#FF3FD7`, açık pembe `#FA8CFF`. Degrade tona göre dizildi.
  - Kullanıcı klavyede gözle "karta benziyor" dedi.
  - "cat" görselinin soluk şeftali tonunu (#FFCABF) da doğru buldu.
- **Windows medya oturumu (GSMTC), salt okuma:** Chrome'da duraklatılmış bir YouTube videosu okundu: başlık, kanal, 24 KB kapak, "çalmıyor".
- **Canlı yükleme (kullanıcı, uygulama günlüğü):**
  - Chrome'da çalan videonun kartı, şarkı "hazır" sayıldıktan sonra 55 ms'de çizildi ve 16 sayfada 2,8 sn'de yüklendi.
  - Kullanıcı kartı "çok güzel" buldu.
  - İlk sürümde hazır sayılma süresi 12 sn idi; kullanıcının isteğiyle 4 sn'ye indi. Yeni sürümde kart 2,7 sn'de yüklendi.
- **Canlı mod kapanınca geri yükleme (kullanıcı):**
  - Kullanıcı önce "cat"i (16 kare, 254 sayfa) elle yükledi (37 sn).
  - Canlı modda bir kart geldi.
  - Canlı modu kapatıp "Önceki görseli geri yükle"yi seçti. "cat" 254 sayfada 37 sn'de geri yüklendi (sayfa başına ~146 ms). Kullanıcı başarılı olduğunu doğruladı.
- **Yükleme sayısı:**
  - Uygulamanın kaydında bugün **11** yükleme var. Bunlara kullanıcının elle yüklemeleri, canlı kartlar ve geri yüklemeler dahil.
  - CLI ile yapılan E1 ve E4 de eklenince toplam **13**.

### Pomodoro ışık katmanı (`04 20`, yalnızca RAM) — 2026-11-10

- **Nasıl:** Gerçek uygulama, CDP ile. Canlı mod kapalıydı, yani ekrana yükleme yapılmadı. Odak 1 dk, mola 1 dk, 2 tur.
- **Sonuç (uygulama günlüğü):**
  - Katman 97 LED'le başladı (`background light layer started leds=97`).
  - Duraklatınca kalan süre 57 sn'de durdu, sürdürünce devam etti.
  - Süre bitince kısa molaya geçti (çubuk yeşil, rakam 1 yandı).
  - "Bitir"den 1 sn sonra (iki yeşil yanıp sönme) katman durdu; klavye kendi efektine döndü (kayıtlı ışık: Kapalı).
  - Işıklar sayfası önizlemesi (`set_live_frame`, yalnız Esc beyaz) Pomodoro katmanının üstüne geldi; önizleme kapanınca Pomodoro katmanı geri geldi.
  - 66 sn'de 15 `04 20` karesi `FF` ile reddedildi (saniyede 20 karede yaklaşık %1). Bir sonraki kare 50 ms sonra gidiyor; bilinen ara sıra `FF`.
- **Bekleyen:** Kullanıcının çubuğu ve renkleri gözle doğrulaması; canlı mod açıkken faz kartlarının yüklenmesi.

### Sistem göstergeleri ışık katmanı (`04 20`, yalnızca RAM) — 2026-11-10

- **Nasıl:** Gerçek uygulama (debug, `target/debug/klavye-studio.exe`), CDP ile. Klavye USB'de. Flash'a yazma yok; ekrana yükleme yok.
- **Okumalar bu PC'de** (`cargo run -p kb-winsys --example probe`): işlemci, ekran kartı (GTX 1650 + Intel UHD, PDH `GPU Engine` sayaçları Türkçe Windows'ta İngilizce adla açıldı), bellek (%80–90), hoparlör (%100, açık), mikrofon (açık). Ekran kartı sayacının ilk açılışı ~1 sn, sonraki okumalar ~1 ms; ses ~9 ms'de açıldı. Not: `Get-Counter '\Processor(_Total)\% Processor Time'` bu makinede İngilizce adla hata verdi (`c0000bb8`); işlemci için PDH değil `GetSystemTimes` kullanılıyor.
- **Sonuç (uygulama günlüğü, UTC):**
  - 22:48:37 göstergeler açıldı: `Gauges` katmanı 97 LED'le başladı. 22:49:06 kapatıldı: "light layer stopped", "no light layer left; keyboard reverts to its saved effect".
  - 22:54:41 göstergeler açıkken (kullanıcının ayarı: işlemci, ekran kartı, bellek, Isı, Karanlık, parlaklık 4) "Klavyede dene": 22:54:43 `Timer` katmanı başladı, 22:54:56 durdu. `Gauges` katmanı sürdü; klavye kayıtlı ışığa dönmedi (beklenen).
  - Kare reddi (`FF`): yeni oturumda yaklaşık 230 sn'de 66 kare (yaklaşık %1,4; saniyede 20 kare). En sık zamanlayıcı denemesi sırasında (20 sn'de 22). Art arda uyarı ("keep failing") yok; bilinen ara sıra `FF`.
- **Kullanıcı:** 22:50'de uygulamada göstergeleri kendisi açtı (bellek dahil). Ardından klavyede denedi (2026-11-11): "Çalışıyor."
- Not: Testlerde sistem ses düzeyine ve mikrofona dokunulmadı; topuz ve mikrofon kullanıcının kendi denemesine bırakıldı.

### Odak zamanlayıcısı: bütün klavyede boşalan ışık — 2026-11-10

- **Nasıl:** Gerçek uygulama, CDP ile. "Klavyede dene" iki kez çalıştırıldı (yalnızca RAM; ekrana yükleme yok). Kullanıcının kayıtlı ayarları önce okundu, sonra aynen geri yazıldı.
- **Sonuç (uygulama günlüğü):**
  - Birinci tur: varsayılanlar (sağdan sola, iki kez yanıp sönme, turuncu), ses açık. Katman 97 LED'le başladı, 12 sn sonra durdu. Ses çalarken hata yok (duyulup duyulmadığını kullanıcı söyleyecek).
  - İkinci tur: soldan sağa, dalga, mor odak. Katman yine 97 LED'le başladı ve 12 sn sonra durdu; klavye kendi ışığına döndü.
  - Art arda kare hatası uyarısı yok.
- **Kullanıcının denemesi (2026-11-10):** Kullanıcı odak zamanlayıcısını klavyede denedi: ışıklar (boşalma, renkler), Windows bildirim sesi, geçiş efektleri ve süreler çalışıyor ("oldukça güzel çalışıyor").
  - Tek istek: boşalan tuşlarda kalan soluk iz %8 "beklemede kalıyor" gibi görünüyordu, %3'e indi. Değişiklikten sonra bir "Klavyede dene" turu klavyede çalıştı (97 LED, 12 sn, sonra kendi ışığına döndü).
  - Canlı mod denemede kapalıydı, yani faz kartları klavyeye yüklenmedi. Uygulamanın kaydındaki son canlı kart 11:50'deki "Çalan şarkı" kartı.
- **Kullanıcının ikinci denemesi (2026-11-10 akşam, canlı mod açık):** Faz kartları klavyenin ekranına geldi (uygulama günlüğü, yerel saat):
  - 20:25 "Odak 1/2" ve 20:26 mola kartı: 16'şar sayfa, 2'şer sn.
  - 20:26:43 Durdur: "Erken bitti · 1 dk" özet kartı (16 sayfa, 2 sn). Kullanıcı bunu ekranda kalan "tur görseli" olarak gördü.
  - 20:28 canlı mod kapatıldı ve "cat" geri yüklendi: 254 sayfa, 37 sn. Hata yok.
  - Bu denemeden sonra ekran sırası ve kendi görseline dönüş yapıldı (DURUM madde 31).
- **Kullanıcının üçüncü denemesi (21:02–21:08, yerel saat):** 21:02 odak ve 21:03 mola kartı yüklendi (16'şar sayfa, 2'şer sn). Bütçe 0,63'e düştü.
  - Sonra hiçbir kart gitmedi; durdurunca dönüş de bütçe bekledi. Kullanıcı ekranın "takılı kaldığını" gördü. Klavye ya da yükleme hatası yok; sebep bütçe (DURUM madde 32).
  - Düzeltmeden sonra uygulama yeniden açıldı (21:20:18). Açılış kontrolü ekranda eski zamanlayıcı kartını buldu ve 15 sn'lik dönüşü kurdu.
  - 21:20:33'te bütçe boşken de dönüş başladı. 21:21:11'de "cat" geri yüklendi: 254 sayfa, 37 sn, hata yok. O sırada Chrome'daki şarkının kartı bütçe bekliyordu; bütçe gelince ya da "Şimdi gönder"le gider.
  - Kullanıcı canlı planını Yoğun'a aldı ve odak zamanlayıcısını canlı modla yeniden denedi: "Sorun çözüldü, çalışıyor." (2026-11-10)

### Sonuçlar

1. **Durum baytı kuralı:** Yalnızca `FF` ret demektir. `00` ve `01` kabul olarak sayılır.
2. **Atomik kare:** Bir `04 20` başlığı gönderildiyse kare her koşulda tamamlanmalıdır. Sürücü artık bunu yapıyor.
3. **Canlı efektler:** Uygulama çalıştığı sürece sürekli akış gerekir. Uygulama kapanınca klavye güvenle kendi ayarına döner.

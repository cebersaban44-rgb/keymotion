# AJAZZ AK980 PRO: USB/HID protokol referansı (FF13 komut kanalı, FF68 ekran kanalı, 2.4G dongle)

> Durum: araştırma belgesi, 2026-11-10. Bu belge topluluk projelerinden ve statik tersine mühendislik notlarından derlenmiştir. Hiçbir komut bu araştırma sırasında cihaza gönderilmedi. Kaynaklar yalnızca **referans** olarak okundu; mimarileri kopyalanmadı.
> Ofset gösterimi: **P[n]** = 64 baytlık yükün (payload) n. baytı, report ID hariç. Kablodaki 65 baytlık tamponda bu bayt **wire[n+1]** konumundadır (wire[0] = report ID 0x00).

## Güven etiketleri

| Etiket | Anlamı |
|---|---|
| **[AK980-HW: Sx]** | Sx kaynağı bunu AK980 donanımında doğrulamış. S1 ve S2'nin "AK980 V2" dediği cihaz, bizim kartla aynı USB kimliğini veriyor: `0C45:8009`, ürün dizgesi "SONiX AK980 PRO", 240×135 ekran, 97 tuş, dongle `0C45:FEFE` (S2 README.md:234-235, S1 KeyboardModel.cs:43-64). Bu yüzden onların donanım testleri bu kart için de geçerli sayılıyor. |
| **[KARDEŞ-HW: Sx, model]** | Aynı SONiX/HFD platformundaki başka bir kartta donanımla doğrulanmış: AK35I V3 MAX, AK820 Pro 8009/800A, AULA F108 Pro, AKS075. |
| **[ÇIKARIM]** | Statik analizden (Ghidra/objdump) ya da kaynaklar arası tutarlılıktan çıkarılmış, donanımda gözlenmemiş. |
| **[ÇELİŞKİLİ: …]** | Kaynaklar birbirini tutmuyor; varyantlar listelenmiş. |

Kaynak kısaltmaları (S1–S12, W1–W9) ve commit bilgileri en sondaki **Kaynaklar** bölümündedir.

---

## 0. Yönetici özeti

1. **Kimlik.** "AK980 V2" üzerinde çalışan iki proje (S1 sonix-keyboard-studio, S2 ajazz-ak980-v2-linux) bizim kartla aynı USB kimliğini görüyor. Komut seti, AK980 PRO resmi sürücüsünün (DeviceDriver.exe, S1'deki "AK980 build") statik analiziyle de örtüşüyor. Pratikte referans cihazımız bu ikisinin test ettiği cihaz.
2. **Çerçeve (framing).** Her komut 64 baytlık bir **Feature** raporudur ve önüne report ID `00` eklenir (toplam 65 bayt). Biçim: `04 OP ARG 00 00 00 00 00 CNT_LO CNT_HI`.
   - **P[8..9] bir bayrak değil, sayaçtır**: ardından gelen 64 baytlık ham veri paketlerinin sayısı. "START flag 0/1" çelişkisi buradan çıkıyor; START'ın verisi olmadığı için doğru değer 0'dır.
   - Okuma (read-back) cevabında **P[3] = 01 kabul**, **FF ret/meşgul** anlamına gelir.
3. **Kapanış sırası.** Işık, per-key ve keymap akışları `04 02` (COMMIT) ve ardından `04 F0` (FINISH) ile biter. Seçenek, makro, saat, ekran ve gerçek zamanlı akışlar **yalnızca `04 02`** ile biter.
   - `04 02` olmadan `04 F0` gönderilirse cevap FF olur ve klavye yaklaşık 2 s komut almaz.
   - Ekran yüklemesinden sonra `04 F0` göndermek yüklemeyi bozar.
4. **Mod numaraları.** 0 = Off, 1 = Static … 19 = Shuttle, 0x80 = per-key custom.
   - S3'ün 0x00 = Static listesi, `.lan` dosyasındaki sıranın (521 Static … 540 LED Off) yanlışlıkla ID sanılmasından kaynaklanan bir kaymadır.
   - Kuyruk baytları **`AA 55`** (P[14..15]). Bu, resmi sürücünün USB yakalamasıyla doğrulandı. `55 AA` ile klavye ACK verir ama veriyi sessizce yok sayar (W1).
5. **Flash'a yazmayan gerçek zamanlı ışık yolu VAR: `04 20`.** Her kare `04 20 …[8]=08` → 8×64 bayt `[idx R G B]` → `04 02` şeklindedir; `04 18` ve `04 F0` yoktur.
   - [AK980-HW: S2], [KARDEŞ-HW: S4 AK35i, S7/S8 AK820].
   - Kayıtlı efekt bozulmuyor. 2 ms aralıkla yaklaşık 40 fps elde ediliyor.
   - Akış durunca ne olduğu çelişkili: bir kaynakta son kare kalıyor, diğerlerinde kayıtlı efekte geri dönülüyor.
6. **Flash'a yazmayan canlı ekran yolu YOK** (FF13/FF68 nesli firmware'de). Her `04 72` yüklemesi harici SPI flash'a yazar ve sayfa başına yaklaşık 145 ms sürer. Ekranda yalnızca saat sayfası ve tek bir yüklenmiş animasyon var.
7. **Pil.** Kablolu modda pil okunamıyor. Yalnızca 2.4G dongle üzerinden `20 01` ile okunuyor [AK980-HW: S1, S2]. BT modunda standart GATT Battery Service okunabilir [ÇIKARIM; S4 AK35i'de doğrulandı].
8. **Tehlikeli baytlar.**
   - `AA 42 89 5A FF 71 62 CC` (HFD bootloader sihirli dizisi).
   - `04 38` (harici flash'a adres 0'dan yazar).
   - 140 kareyi aşan ekran yüklemesi: kardeş kartta menü grafiklerini kalıcı olarak bozdu.
   - BLE `FEE0/FEE1` (WCH OTA).
   - Dongle `0F` / `1F`.
   - ISP PID'leri `0C45:7140`, `7010`, `7040`, `7900`.
   - Ayrıntılar §12'de.

---

## 1. Cihaz tespiti

### 1.1 VID/PID ve arayüzler

| Mod | VID:PID | Arayüz | Usage page / usage | Raporlar | Rol | Güven |
|---|---|---|---|---|---|---|
| Kablolu | `0C45:8009` | MI_00 | 0x0001 / 0x06 | boot klavye | yazma | [AK980-HW: S2 PROTOCOL.md:14-21] |
| Kablolu | | MI_01 | 0x000C (+mouse/system) | consumer, mouse | **knob**, medya tuşları | [AK980-HW: S2] |
| Kablolu | | **MI_02** | **0xFF68 / 0x61** | Output 4097 (ID 0 + 4096), Input 65 | ekran (LCD) sayfaları | [AK980-HW: S1 README.md:54-57, S2] |
| Kablolu | | **MI_03** | **0xFF13 / 0x01** | Feature 65, Input 65, Output 65 | **tüm komutlar** | [AK980-HW: S1, S2; S3 Frida, AK980 PRO] |
| 2.4G dongle | `0C45:FEFE` | MI_00 / 01 / 02 | klavye / mouse / consumer+system+NKRO | | | [AK980-HW: S2 PROTOCOL.md:185-186] |
| 2.4G dongle | | **MI_03** | **0xFF60 / 0x61** | Output 33, Input 33 | pil, saat, (ışık) | [AK980-HW: S1 SonixDongle.cs:5-11, S2] |
| 2.4G dongle | | MI_04 | 0xFF59 / 0x61 (S2) veya 0xFF60 (S5) | 65 / 65 | kullanılmıyor | [ÇELİŞKİLİ: S2 FF59, S5 hid.js:10-11 FF60] |
| Bluetooth | ? | — | vendor kanal yok | — | yalnızca HID | Sizin donanım doğrulamanız; S3 1033.lan id 62 |
| ISP/bootloader | `0C45:7140` | — | — | — | MCU flasher | [KARDEŞ-HW: W5, AK820 Pro HFD80CP100] **ASLA açma** |

Ek notlar:

- **Resmi AK980 PRO sürücüsünün `config.xml` dosyası** (Beta 1.0.0.6): kablolu filtre `VID_0C45&PID_8009&MI_00`, 2.4G filtre `VID_0C45&PID_FEFE&MI_03`. Aynı dosyada bir `XS75T` varyantı `05AC:024F` olarak görünüyor (S3 ak980pro_vendor.md:37-51).
- **Cihaz parametreleri** (`config.xml`): `<screen gif_headlength="256" gif_maxframes="140" gif_count="1" width="240" height="135"/>`, `<light rgb_keyboard="1" default_mode="11" default_brightness="5" default_speed="3" brightness_max="5" speed_max="5"/>`, `<cmd_delaytime value="35"/>` (S3 ak980pro_vendor.md:47-51; S1 LcdAnimation.cs:239-243).
- **2.4G modunda** klavyenin kendi USB arayüzleri kaybolur; kablo yalnızca şarj eder (S1 README.md:99, S2 PROTOCOL.md:34). Dongle yalnızca klavye 2.4G modundayken cevap verir.
- **PID çakışması.** AK35I V3 MAX'ta `0C45:FEFE` **Bluetooth** PnP kimliğidir; dongle orada `0C45:FDFD`, macOS'ta `05AC:024F` görünür (S4 STATIC_REVERSING.md:24, 537, 551). AK820'de de BT `FEFE`, dongle `FDFD` (S7 PROTOCOL.md:13-17).
  - AK980 PRO'da dongle `FEFE` (S3 config.xml + S1/S2 donanım).
  - AK980 PRO BT modunun PID'i bilinmiyor; o da `FEFE` olabilir. **Dongle'ı yalnızca VID/PID ile değil, bus türüyle de ayırt edin**: Windows aygıt yolu `USB\`/`HID\VID_…` ile `BTHLEDevice\`/HOGP UUID `{00001812-…}`. Ayrıca FF60 koleksiyonunun varlığına ve Output uzunluğunun 33 olmasına bakın.
- **MI_03 HID rapor tanımlayıcısı.** S2 hid.rs:367-375'teki test fikstürü: `06 13 FF 09 01 A1 01 15 00 26 FF 00 75 08 95 40 09 02 81 02 09 03 91 02 09 04 B1 02 C0`. Bu, 64 baytlık Input/Output/Feature anlamına gelir. AK820'de (S9 protocol.md:386-387) FF68 tanımlayıcısı `… 96 00 10 75 08 91 02` (4096 Output) ve 64 baytlık Input içerir.

### 1.2 AK980 PRO'yu kardeşlerinden ayırma

| Ölçüt | AK980 PRO / "AK980 V2" | AK820 Pro | AK35I V3 MAX | AULA F108 Pro | AK980 MAX |
|---|---|---|---|---|---|
| PID | 8009 | 8009 (eski) / 800A | 8009 | 800A | **8043, 8050–8054** |
| Ürün dizgesi | **"SONiX AK980 PRO"** (S2 README:234; sizin gözleminiz) | "SONiX AK820" / "AK820" (S7 ak820.py:579; S9 README:57-60) | "USB KEYBOARD" (S4 HIDDiscovery.swift:69) | — | "AK980 MAX" |
| Ekran | 240×135, 140 kare | 128×128 (0.85"), 140 kare | 240×135, 141 kare | 240×135, 141 kare | yok |
| Komut kanalı | FF13 Feature 65 | FF13 (fw ≤1.10); **fw 1.16'da FF68 64 bayt "AA" protokolü + FF67 4104** (S11) | FF13 | FF13 | **FF68 64 bayt Output, "AA" protokolü** (S12) |

- **Tanımlayıcı şekli belirleyicidir.** `FF13` Feature=65 **ve** `FF68` Output=4097 varsa eski "04 protokolü" kullanılıyordur.
  - `FF68` Output=65 ve/veya `FF67` 4104 varsa farklı "AA cmd len addr…" ailesidir (web-sürücü / AK980 MAX / AK820 fw 1.16).
  - Bu iki aileye birbirinin komutlarını **göndermeyin** (S6 COMPATIBILITY.md:21-30; S12 protocol.md:23-26).
  - Usage page tek başına yetmez: `FF68`, 8009'da ekran kanalı, 8043'te komut kanalıdır.
- **AK980 V2 ile AK980 PRO arasında ayırt edici bir özellik yok**: aynı dizge, aynı PID (S2 README.md:234-235).
- **Firmware sürümü**: USB `bcdDevice`, örneğin `0x0130` → "V1.30", `V%X.%02X` biçiminde. Resmi sürücü de sürümü cihaza sormaz, USB enumeration'dan alır [KARDEŞ-HW: S4 FirmwareInfo.swift:31-50, STATIC_REVERSING.md:557].
- **FF13 firmware'inde cihaz-bilgisi komutu yok.** "AA 10 30 …" (GET_DEVICE_INFO) yalnızca AA ailesine aittir. FF13 kartlarında cevap gelmez (S6 COMPATIBILITY.md:25-26; S9 REMAP-RESEARCH.md:128-137).

---

## 2. Taşıma (transport) çerçevesi

### 2.1 Rapor biçimleri

| Kanal | Windows API | Uzunluk | İçerik |
|---|---|---|---|
| MI_03 FF13 komut | `HidD_SetFeature` (≡ IOCTL 0xB0191) | 65 | `00` + 64 bayt yük |
| MI_03 FF13 cevap | `HidD_GetFeature` (≡ IOCTL 0xB0192) | **65** (64 baytlık tampon Windows'ta başarısız olur, S3 ak980-keyboard.md:162-163) | `00` + 64 bayt yankı |
| MI_02 FF68 ekran | `WriteFile` (interrupt OUT; **SET_REPORT değil**) | 4097 | `00` + 4096 bayt sayfa |
| MI_02 FF68 ack | `ReadFile` (Input) | 65 | `00 01 5A 02 00 …` |
| Dongle FF60 | `WriteFile` / `ReadFile` | 33 | `00` + 32 bayt (son bayt checksum) |

> **Uyarı:** FF68 sayfalarını `HidD_SetOutputReport` ile (kontrol aktarımı, SET_REPORT) göndermek kardeş kartın firmware'ini çökertti. Güç verip kesmek kurtardı (W1 hid-protocol.md). `WriteFile` kullanın.

### 2.2 Komut başlığı (her `04 xx` paketi)

| P | Değer | Not |
|---|---|---|
| 0 | `04` | **veri baytı**, report ID değil (S3 ak980pro_vendor.md:227-261, Frida ile AK980 PRO'da düzeltilmiş; S9 protocol.md:464-469) |
| 1 | OP | opcode |
| 2 | ARG | alt-argüman (17 → 01, 72 → slot, …) |
| 3–7 | 00 | |
| 8 | CNT_LO | ardından gelen 64 baytlık veri paketlerinin sayısı |
| 9 | CNT_HI | yalnızca 72 (ekran) için sıfırdan farklı olur |
| 10–63 | 00 | |

- Veri paketlerinin başında işaret yoktur; ham 64 bayt gönderilir.
- [AK980-HW: S1 SonixKeyboard.cs:116-126; S2 device.rs:243-254] [KARDEŞ-HW: S8 proto.rs:170-179 test `04 13 00 00 00 00 00 00 01 00`]

### 2.3 İşlem (transaction) akışları

| Akış | Sıra | Kapanış | Kaynak |
|---|---|---|---|
| Işık efekti | `04 18` → `04 13 00 … [8]=01` → 1 veri → `04 02` → `04 F0` | 02 + F0 | S1 SonixKeyboard.cs:208-218; S2 device.rs:327-342 |
| Per-key mod + tablo | (ışık akışı, mod `80`) + `04 18` → `04 23 … [8]=09` → 9 veri → `04 02` → `04 F0` | 02 + F0 | S1 :236-271 |
| Keymap | `04 18` → `04 11`/`04 27 … [8]=09` → 9 veri → `04 02` → `04 F0` | 02 + F0 (ikisi de ACK'li) | S1 :300-315 |
| Seçenekler | `04 18` → `04 17 01 … [8]=01` → 1 veri → `04 02` | yalnızca 02 | S1 :221-230 |
| Makro | `04 19` (**`04 18` yok**) → `04 15 … [8..9]=N` → 30 ms → N veri → 30 ms → `04 02` | yalnızca 02 | S1 :322-335 |
| Saat | `04 18` → `04 28 … [8]=01` → 1 veri (**ACK'li**) → `04 02` | yalnızca 02 | S1 :343-365; S3 Frida |
| Ekran | `04 18` → `04 72 SLOT … [8..9]=SAYFA` → MI_02'de SAYFA×4096 (her biri ack) → `04 02` | **yalnızca 02, asla F0** | S1 :368-387; W2; S7 |
| Gerçek zamanlı | `04 20 … [8]=08` → 8 veri → `04 02` | **18 yok, F0 yok** | S2 device.rs:376-409; S4; S7 |
| Renk okuma | `04 F5` → GetFeature ×≤10 → 9× GetFeature → `04 02` | 02 | S1 :160-200 |

**ACK okunan paketler** (resmi sürücü davranışı; S1 SonixKeyboard.cs:132-150, S4 STATIC_REVERSING.md:54):

- Okunanlar: `04 18`, komut başlığı, `04 02`; saat akışında veri paketi de; keymap ve per-key akışında `04 F0`.
- Okunmayanlar: veri paketleri; ışık akışının `04 F0`'ı.

### 2.4 Cevap / ACK biçimi

| Durum | Cevap (P) | Anlam | Güven |
|---|---|---|---|
| Komut kabul | `04 OP ARG 01 …` (yankı + P[3]=01) | tamam | [AK980-HW: S1 README:59, S2 PROTOCOL.md:39-41] [KARDEŞ-HW: S4:483, S6 legacy.rs:142] |
| Ret / meşgul | P[3] = `FF` | reddedildi | [KARDEŞ-HW: S6 MACOS-800A.md:162-167] |
| Meşgul (START) | tamamen `00` ya da `04 18 00 FF` | önceki oturum kapanmamış | [KARDEŞ-HW: S6 legacy.rs:162-176] |
| `04 02` cevabı | `04 02 00 01 SUM_LO SUM_HI …` | P[4..5] = duyurudan beri alınan veri baytlarının LE16 toplamı | [KARDEŞ-HW: S7 PROTOCOL.md:36, W1 örneği `04 02 00 01 07 02`, S4:83] |
| Veri paketi | baytların aynen yankısı | durum alanı yok | S4:483 |

- [ÇELİŞKİLİ] S2 (AK980 V2), `04 02`'den sonra P[3]'ün `FF` olduğunu söylüyor (PROTOCOL.md:40). S6, S7 ve W1 (AK820/F108) `01` görüyor. **Yakalamayla doğrulanmalı.**
- Gelecekte kullanmak için: `04 02` cevabındaki toplam alanı, gönderdiğimiz verinin bütünlüğünü kontrol etmeye yarar. S7 uyuşmazlıkta 3 kez yeniden deniyor.
- Uyarı: S9 yanlış çerçeveyle (başında `04` olmadan) gönderdiği her paketin, rastgele opcode'lar dahil, wire[4]=`FF` ile yankılandığını gördü (protocol.md:412-440). Yani **`FF` durum baytı "tanınmadı/ret" anlamına da gelir**; bunu veri sanmayın (bkz. §3, kablolu pil).

### 2.5 Zamanlama ve meşgul tespiti

| Konu | Değer | Güven |
|---|---|---|
| Komut öncesi bekleme | **35 ms** (`cmd_delaytime`), okumadan önce de 35 ms | [AK980-HW: S1 SonixKeyboard.cs:47-48, 102-114; S3 config.xml] |
| 0 ms aralık | sessizce hiçbir şey yapmaz (saat değişmedi) | [AK980-HW: S3 ak980-keyboard.md:85-91, AK980 PRO] |
| Makro verisi | önce ve sonra +30 ms | S1 :329-332 |
| Gerçek zamanlı | komut öncesi/sonrası 5 ms, veri paketleri arasında 2 ms; ≥1 ms şart, aksi halde pipe stall/EPIPE | [AK980-HW: S2 PROTOCOL.md:126-129] [KARDEŞ-HW: S4:529] |
| START yeniden deneme | FF veya boş cevapta 10 × 300 ms | [KARDEŞ-HW: S6 legacy.rs:162-176] |
| F0 COMMIT'siz | FF; ardından yaklaşık 2 s START, başlık ve veri reddedilir | [KARDEŞ-HW: S6 legacy.rs:185-188, MACOS-800A.md:162-167] |
| GetFeature hatası | EPIPE/ETIMEDOUT durumunda 5/10/15 ms bekleyip 3 deneme | [AK980-HW: S2 hid.rs:189-204] |
| Driver çok parçalı gönderim | hata olursa 10 deneme | [ÇIKARIM: S4:56] |

- Gerçek zamanlı göndericide (driver `0x44fc90`) okunan P[3]=`FF` ise paket hemen bir kez yeniden gönderilir. Toplam 100 ms'i aşarsa kare atlanır (S4:529).
- Tek yazar kuralı:
  - Tüm işlem (START…COMMIT) atomik tutulmalı; S2 flock kullanıyor.
  - Resmi uygulama açıldığında saat **ve sayfa** paketi gönderir (S1 README.md:14, 24). Resmi uygulamayla birlikte çalıştırmayın.

---

## 3. Okuma / sorgu komutları

| Komut | Kanal | Paket | Dönen | Yan etki | Güven |
|---|---|---|---|---|---|
| USB tanımlayıcıları | OS | `bcdDevice`, iProduct, iManufacturer, HidP_GetCaps | firmware sürümü, model dizgesi | yok | [KARDEŞ-HW: S4] |
| **Per-key canlı renk** | FF13 | `04 F5` (P[8]=00, S1; S4/S8 `09` gönderiyor) → echo (P[3]==01 ise veri gelir) → ≤9 × GetFeature (ilk okuma echo'yu tekrarlayabilir, atla) → `04 02` | 576 bayt `[idx R G B]×144`, **o an ekranda görünen** renkler, ~6 bit (FF → FC okunur) | sürücü bunu rutin olarak yokluyor; sonda `04 02` var | [AK980-HW: S2 "cevap veriyor"] [KARDEŞ-HW: S1 F108, S4 AK35i] |
| **Pil** | dongle | `20 01` | `20 01 00 PCT` (0 = cevap yok) | yok | [AK980-HW: S1 SonixDongle.cs:58-65, S2 PROTOCOL.md:214] |
| Dongle bilgi probe | dongle | `02 00` | `02 00 00 40 30 00 00 VID_LE PID_LE VER_LE FF FF` | yok | [KARDEŞ-HW: S4:477 (`…45 0C 09 80 30 01`), S8 proto.rs:201-212] |
| Profil tuş bildirimi | FF13 Input | (gelen) `05 A6 FF 01` / `05 A6 FE 02` | durum biti | — | [ÇIKARIM: S4:245, S3 :461-475] |
| Makro/başlatıcı bildirimi | FF13 Input | (gelen) `05 xx ID` (xx≠A6) | host'ta çalıştırılacak program/URL ID | — | [ÇIKARIM: S4:245] |

### Var olmayan veya güvenilmez okumalar

- **Kablolu pil (`20 01`, başında `04` yok)**: cevap wire[4]=`FF`. S3 bunu "kablolu+dolu" sanıp "hardware-confirmed" etiketi koymuş (ak980-keyboard.md:162-163); S6 da "wired/full sentinel" diyor. Gerçekte bu, tanınmayan paketin genel `FF` durum yankısıdır (S9 protocol.md:437-440).
  - Firmware'in USB tarafında pil kodu yok (S4:428, 435-440).
  - S2 de pili yalnızca dongle'da buldu. **Göndermeyin.**
- **Mevcut ayar okuma** (efekt, keymap, makro, seçenek): resmi sürücüde yok (S4:87; W1 "write-only"). `F5` animasyonlu modda canlı kareyi döndürür, kayıtlı ayarı değil.
- **Statik analizle bulunmuş okuma opcode'ları** (AK35i firmware V1.17, dispatch `0x5012`/`0x515C`; S4 STATIC_REVERSING.md:408-420): `04 10` base keymap, `04 26` Fn keymap, `04 12` ışık, `04 14` makro, `04 16` sistem, `04 22` per-key, `04 05` bilgi bloğu (`40 30 00 00 VID PID VER`).
  - Bunları resmi sürücü **hiç göndermiyor**; donanımda hiç denenmemiş. [ÇIKARIM]
  - Yazma/okuma çiftleri simetrik ve ayar okuma için çok değerli olabilirler. Ancak "sürücüde olmayan komutu gönderme" kuralı (S1 agents.md:26; S4 SAFETY_MODEL.md:7-8) gereği **ancak yakalama/uzman kararından sonra, yedekli bir test planıyla** denenmeli.
- **Mevcut profil**: cihazda profil yok. Profiller yalnızca sürücünün SQLite veritabanında (S4:81; S2 profiles.rs:29-55).

---

## 4. Aydınlatma

### 4.1 Efekt listesi (ID = P[0])

Firmware sırası, AK980 build `language/1033.lan` 521..540 (S1 Lighting.cs:22-45). Çince adlar AK980 PRO Beta 1.0.0.6 `2052.lan` dosyasından (S3 ak980pro_vendor.md:357-389). `caps` = sürücü DB'sindeki `config_func`.

| ID | EN (AK980 sürücüsü) | ZH (AK980 PRO 2052.lan) | caps | Desteklenenler |
|---|---|---|---|---|
| 0x00 | LED Off | 关闭背光 | 0x00 | — |
| 0x01 | Static | 常亮 | 0x31 | parlaklık, renk, rainbow (hız yok) |
| 0x02 | Single On | 单点亮 | 0x33 | reaktif |
| 0x03 | Single Off | 单熄灭 | 0x33 | reaktif |
| 0x04 | Glittering | 繁星点点 | 0x33 | |
| 0x05 | Fluttering (S3/F108: "Falling") | 漫天飞舞 | 0x33 | |
| 0x06 | Colourful | 万紫千红 | 0x03 | renk yok |
| 0x07 | Breath | 动感呼吸 | 0x33 | |
| 0x08 | Spectrum | 光圈循环 | 0x03 | renk yok |
| 0x09 | Outward | 彩浪涌动 | 0x33 | |
| 0x0A | Scrolling | 万彩纵横 | 0x3B | 4 yön grubu |
| 0x0B | Rolling (**fabrika varsayılanı**) | 寂静流光 | 0x37 | 2 yön |
| 0x0C | Rotating | 峰回环绕 | 0x37 | 2 yön |
| 0x0D | Explode | 一触即发 | 0x33 | |
| 0x0E | Launch | 一触惊艳 | 0x33 | |
| 0x0F | Ripples | 涟漪扩散 | 0x33 | |
| 0x10 | Flowing | 川流不息 | 0x37 | 2 yön |
| 0x11 | Pulsating | 重峰叠叠 | 0x33 | |
| 0x12 | Tilt | 斜风细雨 | 0x37 | 2 yön |
| 0x13 | Shuttle | 来回穿梭 | 0x33 | |
| 0x80 | (Custom / per-key) | — | — | §5 |

- caps bitleri: `01` parlaklık, `02` hız, `04` yön grubu-1 (0/1), `08` yön grubu-2 (2/3), `10` RGB rengi, `20` rainbow/colorful (S1 Lighting.cs:3-14; S4:148).
- [ÇELİŞKİLİ] **Numaralama.**
  - S3, 0x00=Static … 0x13=LED Off diyor. Bu, `.lan` sırasının bir kaydırılmış okumasıdır.
  - S1, S2, S4, W1, W4 ve S8'in resmi sürücü yakalaması (statik mavi = `01 00 00 FF … 05 03 … AA 55`, S8 proto.rs:305-310) **0 = Off, 1 = Static** olduğunu gösteriyor. S3'ü bu konuda yok sayın.
- S3, 1033.lan 500–519'da "Stellar Epoch, Photon Strike, … Hourglass of Time" adlı ikinci bir 20'li liste buluyor ve bunları "profil sahne ön ayarı" olarak yorumluyor [ÇIKARIM; açık soru].
- Doğrulanma durumu:
  - Efektlerin cihazda görsel doğrulaması: S2 "gönderildi ve ACK alındı", S1 "both (AK980 + F108)".
  - S4 AK35i'de mod 1 ve 16 görsel olarak doğrulandı.

### 4.2 Işık veri paketi (`04 13` sonrası tek veri paketi)

| P | Alan | Değer |
|---|---|---|
| 0 | MODE | 0x00–0x13, 0x80, (yan ışık 0x20–0x29) |
| 1 | R | |
| 2 | G | |
| 3 | B | |
| 4–7 | 00 | |
| 8 | RAINBOW | 0 tek renk, 1 colorful/rainbow |
| 9 | BRIGHT | 0–5 (0 = sönük) |
| 10 | SPEED | 0–5 (0 en yavaş, 5 en hızlı; S4'te donanımla doğrulandı) |
| 11 | DIR | yön kodu (aşağıda) |
| 12–13 | 00 | |
| 14 | `AA` | |
| 15 | `55` | |
| 16–63 | 00 | |

- **Örnek** (S2 golden.json:467, AK980 V2'de doğrulandı): `02 FF D2 00 00 00 00 00 00 04 01 02 00 00 AA 55` = mod 2, renk FFD200, parlaklık 4, hız 1, yön 2.
- **Off**: yalnızca P[0]=00 ve P[14..15]=`AA 55` (S1 Lighting.cs:106-123; S2 lighting.rs:125; S4:109).
  - S7 bir yan etki bildiriyor: AK820'de mod 00'dan sonra Fn+X ışığı geri açamıyor.
- **Static**: mod 01 [AK980-HW: S1/S2; resmi yakalama S8].
  - [ÇELİŞKİLİ] S6, S7 ve S9 "static = breath (07) hız 0", "off = single-on (02) parlaklık 0" kullanıyor. Bu araçlar aynı zamanda `55 AA` kuyruğunu kullanıyor; davranış farkı muhtemelen bundan.
- **Kuyruk**: `AA 55` [AK980-HW: S2 device.rs:59; resmi USB yakalaması S8 proto.rs:305-310 ve W1].
  - [ÇELİŞKİLİ] S10, S9, S7 ve S6 `55 AA` kullanıyor. W1: "yanlış sırada ACK gelir ama veri yok sayılır". **AA 55 kullanın.**
- **Yön** [ÇELİŞKİLİ]:
  - S1/S2 AK980: 0 = Left, 1 = Down, 2 = Up, 3 = Right (S2 lighting.rs:58). S10, S9 ve S7 aynı.
  - S5: 0 Left, 1 **Up**, 2 **Down**, 3 Right.
  - S4: modlar 11, 12, 16 ve 18 için {0,1}; mod 10 için {0,2,3}.
  - S8: yalnızca 0/1 (0 = soldan sağa/aşağı, 1 = ters).
  - **Yakalama gerekiyor** (özellikle mod 10).
  - [AK980 PRO-HW: bu proje, 2026-11-10] Ölçüldü (`kb direction-probe`, F5 kareleri): her yönlü efektte iki yön; kod 2 = 1, kod 3 = 0. Mod 10 dikey (0 aşağı), 11 yatay (0 sağa), 12 dönme (0 saatin tersi), 16 LED sırasıyla akan katar (0 ileri), 18 çapraz (0 sağ yukarı). Ayrıntı: `docs/donanim-dogrulama.md`, "Efekt yönleri".
- **Varsayılanlar**: mod 11, rainbow 1, parlaklık 5, hız 3, yön 0. Sürücü rengi kırmızı (S3 config.xml; S4:152).
- **Beyaz dengesi**: (255,255,255) pembe görünüyor. Bir birimde R %78, G %100, B %95 beyaz verdi. Bu host tarafında yapılan bir düzeltme (S2 PROTOCOL.md:217).

### 4.3 Yan ışık / logo ışığı

- **Opcode**: `04 13` akışı, P[0] = yan ışık modu, yalnızca `AA 55` kuyruğuyla (S1 SonixKeyboard.cs:277-291; S2 golden.json:1032-1067).
- **Mod numaraları** [ÇELİŞKİLİ]:
  - S1/S2 (stok uygulamanın combo item verisi): `20` Flowing, `21` Red, `22` Yellow, `23` Green, `24` Ice Blue, `25` Blue, `26` Pink, `27` White, `28` Neon, `29` Off (S1 Lighting.cs:76-89).
  - W1 (F108): `1F`…`28` (bir kayma).
  - Adlar 1033.lan 660–670.
- **AK980'de görünür bir etki yok.** S1 "sent, no visible effect on either"; S2 "hidden in its UI, no visible effect on the AK980" [AK980-HW negatif sonuç].
- **Logo ışığı** (ok tuşlarının yanındaki):
  - Adreslenemiyor. Gerçek zamanlı tablodaki kullanılmayan bütün indeksler denendi, değişmedi (S2 PROTOCOL.md:131-132).
  - Yalnızca firmware kombinasyonuyla kontrol ediliyor: **Fn+Z** aç/kapa. Fn+Z ile kapatıldıktan sonra gönderilen ışık komutları onu kapalı bırakıyor; güç kesilince hatırlanmıyor (S2 README.md:367-370; S1 KeyboardModel.cs:58).
- [AÇIK SORU] AK980 PRO'daki "side light strip" bu logo ışığı mı, yoksa ayrı bir şerit mi? Resmi AK980 PRO sürücüsünün `layouts/rgb-keyboard.xml` dosyasındaki `sidelight=` özniteliği yakalanmalı.

### 4.4 Per-key custom mod

Önce ışık akışı şu veriyle gönderilir: `80 00 00 00 00 00 00 00 00 BR 00 00 00 00 AA 55` (P[9] = parlaklık). Ardından `04 23` tablosu gelir (§5) [AK980-HW: S1 :236-271, S2 golden.json:478].

### 4.5 Gerçek zamanlı ışık (real-time / music / ambilight): FLASH'A YAZMAYAN YOL

**Komut** (resmi sürücü fonksiyonları `0x428110` "real-time" sayfası, `0x432e70` "music" sayfası; S4:73-74):

```
04 20 00 00 00 00 00 00 08 00 …   (GetFeature ile echo oku; P[3]=01 değilse kareyi atla, FF ise 1 kez yeniden gönder)
8 × 64 bayt ham veri               (ACK yok, paketler arası ≥1 ms, tercihen 2 ms)
04 02 00 …                         (echo oku)
```

**Özellikler:**

- `04 18` yok, `04 F0` yok, 35 ms bekleme yok. Sürücü akıştan önce veya sonra mod değiştirmiyor (S4:529).
- **Veri**: 512 bayt = 128 kayıt × `[LED_IDX, R, G, B]`. AA 55 kuyruğu yok.
  - [ÇELİŞKİLİ] **Konumlandırma.** S2 kayıtları idx×4 konumuna yazıyor, kullanılmayan kayıtlar `00 00 00 00`. Resmi sürücü (S4, S7, S8, W3) ise kayıtları düzen sırasında art arda yazıyor.
  - Her kayıt kendi idx'ini taşıdığı için firmware muhtemelen idx baytını kullanıyor. **idx×4 konumlu yazım iki yorumu da karşılar** [ÇIKARIM].
- Yalnızca LED indeksi < 128 olanlar adreslenebilir. AK980'deki en büyük indeks 123 (S2).
- Parlaklık host tarafında uygulanır: `(renk × parlaklık) >> 8` (S4:314).
- `rgb_keyboard=0` olan kartlarda sayaç 3 ve 0xC0 bayt (her LED için 1 bayt) kullanılır. AK980 PRO'da `rgb_keyboard="1"`, dolayısıyla **8**.
- **Flash'a yazılmıyor:**
  - S2 PROTOCOL.md:126: "Nothing is written to flash" [AK980-HW, 2026-10-28].
  - S4 PROTOCOL_STATUS.md:14: 15 s tam mavi akış, 219 kare ACK'lendi, kayıtlı efekt değişmedi [KARDEŞ-HW AK35i].
  - S7 PROTOCOL.md: "not saved" [KARDEŞ-HW AK820].
  - [ÇIKARIM, güçlü] Her karedeki `04 02` flash'a yazmıyor; yalnızca "uygula" anlamında (S4:52). Resmi müzik modu bunu saniyede ~30 kez sürekli gönderiyor; flash'a yazsaydı saatler içinde aşınırdı. Ancak `0x20` handler'ının firmware disassembly'si yayınlanmış değil.
- **Hız:**
  - S2 (AK980): bir kare 3–15 ms; 1 ms aralıkla ~70 fps, 2 ms aralıkla ~40 fps. Uygulama 30 fps kullanıyor.
  - Resmi sürücü ~23 fps (S7), en az 30 ms döngü (S4).
  - macOS'ta 12–15 fps (S4).
  - Bitişik raporlar (<1 ms) EPIPE/stall'a yol açar.
- **Akış durunca** [ÇELİŞKİLİ]:
  - S2 (AK980): "son kare bir sonraki kareye ya da bir yapılandırma komutuna kadar kalır".
  - S4 (AK35i) ve S7 (AK820): "durunca kayıtlı efekte hemen geri döner".
  - W3 (OpenRGB F108 fork) her 2000 ms'de `04 20 … 08` keepalive gönderiyor; bu bir zaman aşımına işaret ediyor.
  - **Yakalama ve testle netleşmeli.** Güvenli yol: akış bitince kayıtlı efekti host'tan bir kez yeniden göndermek (S2 engine.rs:485-498). Ancak bu bir flash yazımıdır; yalnızca gerekiyorsa yapın.
- **Resmi sürücünün host tarafı gerçek zamanlı efektleri** (firmware'e mod olarak gitmiyor, karelerle çiziliyor; S3 :70-72, S4 DriverLightEffects.swift:5-13):
  - ID'ler: 1 Static On, 4 Starlight, 5 Fluttering, 7 Dynamic Breathing, 9 Colorful Fountain, 10 Rainbow Wave, 11 Following Currents, 12 Peak Revolving, 0 Turn Off.
  - Hız 1–15; adım = 16 ms × max(15−hız, 1).
- **Müzik ritmi** (kablolu): aynı `04 20` karesi; ses analizi host'ta yapılır. Tamamen karanlık kareler gönderilmez.
  - Modlar (S4 DriverMusic.swift:6-18): 0 kapalı, 1 yeşil/sarı/kırmızı, 2 rainbow, 3 ters rainbow, 4 gradyan, 5 spektrum döngüsü, 6 beyaz, 7 kırmızı, 8 turuncu, 9 sarı, 10 mavi, 11 yeşil, 100 ekran ambilight.
  - 2.4G için bkz. §11 `0B`.
- **Firmware kalıcı yazımları:** `04 13` ve `04 23` her gönderimde MCU iç flash'ına yazar; kalıcıdır (S4:485). **Akış için asla `04 13`/`04 23` kullanmayın.** S8 de dongle ışık komutunu rate-limit ediyor (README.md:72).

### 4.6 Oyun ışığı setleri (Fn+1/2/3) ve firmware kombinasyonları

- **Fn+1/2/3 "game lighting" hiçbir kaynakta yok.** Sürücünün 1033.lan özellik ağacında da yer almıyor (S3 :56-89) [AÇIK SORU].
- AK980 için belgelenmiş firmware kombinasyonları (S1 KeyboardModel.cs:55-63; S2 gui lighting.rs:454-460):

| Kombinasyon | İşlev |
|---|---|
| Fn+Z | logo ışığı |
| Fn+X | tüm ışıklar aç/kapa |
| Fn+Del | ekran aç/kapa |
| Fn+knob basma | knob: ses ↔ ekran menüsü |

- Şarj olurken Fn tuşunun kırmızı yanması ve Num Lock vurgusu firmware tarafından çiziliyor; bunlar için komut yok.

---

## 5. Per-key RGB ve LED indeks haritası

### 5.1 Paket

1. Mod `80` ışık akışı (§4.4).
2. `04 18` → `04 23 00 00 00 00 00 00 09 00` → 9 × 64 bayt tablo → `04 02` → `04 F0`.

| Tablo ofseti | İçerik |
|---|---|
| idx×4 + 0 | idx |
| idx×4 + 1..3 | R, G, B |
| kullanılmayan | 00 (FF değil) |
| 574–575 | `AA 55` (giriş 143'ün G/B baytlarının üzerine yazılır → **en büyük idx 142**) |

- Kaynaklar: [AK980-HW: S1 SonixKeyboard.cs:236-271; S2 golden.json:474-495, 2026-10-30'da WASD kırmızı ve Esc yeşil olarak görsel doğrulandı] [KARDEŞ-HW: S6 PER-KEY-RGB.md:51-69, S4:177].
- Tabloda bulunmayan tuşlar siyah olur (tamamı değiştirilir).
- [ÇELİŞKİLİ] S3'ün "per-key = `0x20 0x04`, 0xC0 bayt" ve "0x23 = makro buffer" iddiaları, gerçek zamanlı akışın (`04 20 [8]=03/08`) yanlış okunmasından geliyor. Doğru opcode `04 23`.
- Mono varyant (`rgb_keyboard=0`): her LED için 1 bayt, toplam 0xC0, sayaç 3 (W1). AK980 PRO'da geçerli değil.

### 5.2 AK980 LED / tuş indeks haritası

Kaynak: stok `layouts/rgb-keyboard.xml`. S1 `ak980-layout.json` ve S2 `data/layout.json` birebir aynı (yalnızca bir etiket yazımı farklı). LED indeksi = keymap indeksi (S2 layout.rs:21-23; S4:226).

| Satır | Tuş = LED indeksi (ondalık) |
|---|---|
| 0 | Esc 1, F1–F12 = 2–13, Del 119 (ekran ve knob numpad'in üstünde, indeksleri yok) |
| 1 | ` 19, 1–0 = 20–29, - 30, = 31, Backspace 103, Home 117, NumLock 32, Num/ 33, Num* 34, Num- 122 |
| 2 | Tab 37, Q W E R T Y U I O P = 38–47, [ 48, ] 49, \ 67, PgUp 118, Num7 50, Num8 51, Num9 52, Num+ 123 |
| 3 | Caps 55, A S D F G H J K L = 56–64, ; 65, ' 66, Enter 85, PgDn 121, Num4 68, Num5 69, Num6 70 |
| 4 | LShift 73, Z X C V B N M = 74–80, , 81, . 82, / 83, RShift 84, Up 101, Num1 86, Num2 87, Num3 88, NumEnter 106 |
| 5 | LCtrl 91, Win 92, LAlt 93, Space 94, Fn 96 (HID kodu 0xAF, sürücüye özgü), RCtrl 98, Left 99, Down 100, Right 102, Num0 104, Num. 105 |

- **97 tuş**. RAlt (95) ve Menu (97) yok.
- Kullanılmayan indeksler: 0, 14–18, 35, 36, 53, 54, 71, 72, 89, 90, 95, 97, 107–116, 120, 124–127 (S2 PROTOCOL.md:131-132).
- Izgara satır başına 18 indeks (satır tabanları 0/18/36/54/72/90). Büyük tuşlar ve sağ kenar tuşları ızgara dışına (≥101) taşınmış [ÇIKARIM].
- HID kodları standart sayfa 0x07 değerleri (Esc 0x29, F1 0x3A …).

---

## 6. Keymap (base ve Fn katmanı)

### 6.1 Paket

`04 18` → `04 11` (base) veya `04 27` (Fn), P[8]=09 → 9×64 = 576 bayt → `04 02` (ACK) → `04 F0` (ACK).

- 144 giriş × 4 bayt; giriş idx×4'te. Bayt 574–575 = `AA 55`. Geçerli idx 1..142.
- **Bir katmanı yazmak katmanın tamamını değiştirir**: tabloda olmayan tuş firmware varsayılanına döner.
- [AK980-HW: S1 :300-315; S2 PROTOCOL.md:212, 2026-10-30'da base ve Fn katmanı doğrulandı] [KARDEŞ-HW: S4:514-516 replug sonrası kalıcı; S7].
- [AK980 PRO-HW: bu proje, 2026-11-10] Ölçüldü: iki katman da yazıldı, `04 02` toplamı 576 baytla tuttu, `04 F0` yankısı geldi. `02` tuş + değiştirici, `03` consumer, `01 01` fare, `05 03` devre dışı ve sıfır tabloyla varsayılana dönüş doğrulandı. Fare `08` Windows'ta 4. düğme (geri). Ayrıntı: `docs/donanim-dogrulama.md`, "Tuş atama".

### 6.2 Giriş kodlaması

| Giriş | Anlam | Örnek |
|---|---|---|
| `00 00 00 00` | firmware varsayılanı (Fn işlevleri dahil) | |
| `05 03 00 00` | devre dışı | |
| `02 MODS CODE 00` | HID tuşu + modifier; MODS: 01 Ctrl, 02 Shift, 04 Alt, 08 Win, 10/20/40/80 sağ eşleri | Ctrl+Shift+A = `02 03 04 00`; Win+L = `02 08 0F 00` |
| `02 BIT 00 00` | tek modifier tuşu; BIT = 1 << (kod − 0xE0) | Ctrl = `02 01 00 00` |
| `01 01 BTN 00` | fare tuşu: 01 sol, 02 sağ, 04 orta, 03 çift tık, 10 tuş-4, 08 tuş-5 | |
| `01 03 DIR 00` | tekerlek: 01 yukarı, FF aşağı (değerden bağımsız olarak bir çentik) | |
| `03 LO HI 00` | consumer usage (LE) | §6.3 |
| `06 SLOT MODE COUNT` | makro: SLOT 0 tabanlı (0–99); MODE 0 bir kez, 1 COUNT kez (1–255), 2 tekrar basana kadar, 3 basılı tutulduğu sürece, 4 "click to play"; COUNT yalnızca MODE 1 iken | `06 02 01 05` |
| `07 K1 K2 K3` | aynı anda en fazla 3 tuş (modifier'lar E0–E7 olarak yazılır) | `07 04 05 06` |
| `05 02 ID 00` | program/URL/metin: **yalnızca host uygulaması çalışırken** işler; firmware bir Input raporu gönderir | (sunmayın ya da kendi servisimizle işleyin; tuşla uygulama başlatma fikri için `docs/DURUM.md` P2) |

- Kaynaklar: S1 README.md:77-89, KeyMap.cs:8-15, 105-148; S2 keymap.rs:618-637; S4 Keymap.swift.
- Notlar:
  - S7 AK820'de 08/10 fare tuşlarının çalışmadığını bildiriyor.
  - Fn tuşunun kodu `0xAF` (driver VK 0xFE → 0xAF). **Fn'yi yeniden atamayın** (S8 main.rs:261-262).
  - Kendi Fn işlevi olan tuşlar Fn katmanı atamasını yok sayabilir; AK980'de Fn+Home'da görüldü (S1 README.md:89).
  - AK820'de Fn katmanı Esc, Bksp, Home, \ ve Win için kullanılamıyor; Del yok sayılıyor (S7 PROTOCOL.md) [KARDEŞ-HW].
  - F13–F24 (0x68–0x73) fiziksel olarak yok ama gönderilebilir. Host makroları için tetik olarak kullanılabilirler (S2 keymap.rs:337-340).

### 6.3 Consumer kodları (stok sürücünün gönderebildikleri)

| İşlev | Kod |
|---|---|
| Play/Pause | CD |
| Stop | B7 |
| Prev | B6 |
| Next | B5 |
| Vol+ | E9 |
| Vol− | EA |
| Mute | E2 |
| Calculator | 0192 |
| Mail | 018A |
| Media player | 0183 |
| My Computer | 0194 |
| Browser home / refresh / stop / back / forward / favorites / search | 0223 / 0227 / 0226 / 0224 / 0225 / 022A / 0221 |

Kaynak: S1 KeyMap.cs:398-415. Varsayılan Fn+F1 = 0x183 (S4:515).

### 6.4 Özel firmware işlevleri

BT kanal değiştirme, 2.4G/USB seçimi, ışık kısayolları ve pil göstergesi gibi işlevler için **bilinen bir giriş kodu yok**. Bunlara yalnızca `00 00 00 00` (varsayılan) ile dönülebilir [AÇIK SORU].

### 6.5 Varsayılana dönme

Cihazda bir reset komutu yok. Stok "Reset keyboard" iki katmanı da sıfırlardan oluşan girişler + `AA 55` ile yeniden gönderir (S4:85, 583; S7 PROTOCOL.md "Factory reset"). Donanım kombinasyonları için bkz. §8.

---

## 7. Makrolar

### 7.1 Akış

`04 19` (`04 18` gönderilmez) → `04 15 … [8..9]=PAGES` → 30 ms → PAGES×64 → 30 ms → `04 02` (F0 yok) [AK980-HW: S1 SonixKeyboard.cs:322-335; S2 2026-10-30 "ok" yazan makro].

### 7.2 Görüntü (image) biçimi

| Ofset | İçerik |
|---|---|
| 0–399 | 100 × 4 bayt dizin: `OFS_LO OFS_HI 00 00` (kaydın mutlak ofseti); tanımlı ama boş makro `FF FF FF FF`; kullanılmayan `00 00 00 00` |
| 400… | kayıtlar: `[olay sayısı LE16][6 × 00][olaylar…]` |

- Her adım iki 4 baytlık olaydan oluşur:
  - Eylem `00 00 CODE TYPE`. TYPE: `B0` tuş bas, `30` tuş bırak, `90` fare bas, `10` fare bırak. CODE = HID kodu ya da fare maskesi (1 sol, 2 sağ, 4 orta).
  - Bekleme `MS_LO MS_HI 00 50` (≥10 ms, ≤65535).
- `PAGES = boyut/64 + 1` (boyut 64'ün katıysa), değilse `+2`. Son sayfanın son iki baytı `AA 55`.
- Boş tablo = 8 sayfa sıfır, 510–511'de `AA 55` (S4:584).
- **Limitler:**
  - En fazla 100 makro.
  - Stok sürücü `(adım + makro) × 8 > 0xA8C` olunca reddeder (S1 Macro.cs:125-139).
  - Tampon en fazla 0xE00 = 3584 bayt (56 sayfa) (S7; S4).
- **Atama:** oynatma modu ve sayısı makro tablosunda değil, tuş girişinde (`06 SLOT MODE COUNT`) tutulur. Bir makro silinince sonraki slotlar kayar; makro görüntüsü ve iki katman yeniden yazılmalıdır (S2 config.rs:116-138).
- Tek blok iki katman için ortaktır (S7).
- Golden örnek (S2 golden.json:556-575): dizin `90 01 00 00 | E8 01 00 00 | 20 02 00 00 | FF FF FF FF | 48 02 00 00`; ilk kayıt `14 00 00 00 00 00 00 00 | 00 00 0A B0 | 19 00 00 50 …`.

---

## 8. Seçenekler (`04 17`)

### 8.1 Akış

`04 18` → `04 17 01 00 00 00 00 00 01 00` → 1 veri → `04 02` (F0 yok).

| P | Alan | Değerler |
|---|---|---|
| 0 | 00 | |
| 1 | 01 | sabit |
| 2 | Alt+Tab kilidi* | 0/1 |
| 3 | Alt+F4 kilidi | 0/1 |
| 4 | Win kilidi* | 0/1 |
| 5 | Fn switch | 0 basılı tut, 1 toggle |
| 6 | Uyku | 0 asla, 1 = 1 dk, 2 = 5 dk, 3 = 30 dk (yalnızca pilde) |
| 7 | 00 | |
| 8 | Tuş tepki süresi (debounce) | 1–5 (kablolu ≈ 2–3 / 5–6 / 8–9 / 13–14 / 17–18 ms) |
| 9–61 | 00 | |
| 62–63 | `AA 55` | |

- Fabrika verisi: `00 01 00 00 00 00 02 00 02 00 … AA 55` (S4:212).
- Kaynaklar: S1 Lighting.cs:48-74; S2 lighting.rs:176-182; S4:181-212.
- \* [ÇELİŞKİLİ] **P[2] ile P[4]'ün sırası:**
  - S1, S2 ve S4: [2]=AltTab, [4]=Win.
  - S7 (AK820): [2]=Win, [4]=AltTab.
  - S2'nin "Win lock doğrulandı" testinde [2] ve [4] birlikte 1'di (golden.json:510), dolayısıyla ayırt edici değil.
  - S4 AK35i'de [4] tek başına Win'i kilitlemedi, [3]+[4] birlikte kilitledi.
  - **Her kutu tek tek işaretlenerek yakalama yapılmalı.**
- [ÇELİŞKİLİ] S5, S10 ve S9 "uyku = P[8]" diyor. Bu yanlış; P[8] tepki süresi (S7 ak820.py:121).
- S5'teki `04 19` "response time", `04 50` factory reset ve `04 51` keyboard reset uydurmadır/doğrulanmamıştır. **`04 19` makro silme/başlatma opcode'udur.** Bunlar engellenmeli (§12).
- **Polling rate, NKRO, Win/Mac:** bu ailenin sürücüsünde komut yok.
  - AK35i'de Win/Mac bir donanım anahtarı. Mac modunda firmware Win ile Alt'ı değiştiriyor; kilit seçenekleri etkisiz kalıyor (S4:493; S7).
  - AK980 PRO'da Win/Mac geçişinin nasıl yapıldığı bilinmiyor [AÇIK SORU].
- **Fabrika sıfırlama** (yalnızca belgeleme; **tehlikeli**):
  - USB'de bir komutu yok. Stok "Factory Reset" yalnızca varsayılan profili yeniden gönderiyor: boş katmanlar, makro yok, mod 0B rainbow, parlaklık 5, hız 3, varsayılan seçenekler (S7; S4:85).
  - Donanım kombinasyonu: F108'de Fn+Esc 3 s (S1), AK35i/AK820'de Fn+Space 3–5 s. AK980 PRO için doğrulanmadı.
  - Dongle'da firmware `0F` (factory restore) ve `1F` (keymap reset) var (§11) → engelleyin.

---

## 9. Knob

- Ses için consumer raporları gönderir, mute için basma; kablolu MI_01 (usage page 0x000C) üzerinden (S2 PROTOCOL.md:17).
  - AK35i BT tanımlayıcısında bir "Dial" usage'ı da var (S4:551) [ÇIKARIM].
- **Yeniden atanamaz**: hiçbir kaynakta knob için bir komut ya da keymap indeksi yok. S2 GUI'si knob'u yalnızca çiziyor (indeks 0).
- Fn+knob basma, knob'u "ses" ve "ekran menüsü" arasında değiştiriyor. Ekran menüsünde saat sayfası ve yüklenmiş animasyon seçilebiliyor (S1 KeyboardModel.cs:61; S2 README.md:359, PROTOCOL.md:112-114).
- [AÇIK SORU] MI_01 HID tanımlayıcısında knob hangi usage'ları üretiyor? Knob'un keymap tablosunda gizli bir indeksi var mı?

---

## 10. TFT ekran

### 10.1 Geometri

- 240×135, RGB565 little-endian, satırlar yukarıdan aşağı.
- Piksel = `((r>>3)<<11)|((g>>2)<<5)|(b>>3)`; kırmızı kabloda `00 F8` olarak gider (S2 lcd/encode.rs:21-36; S4:405).
- En fazla 140 kare (`gif_maxframes="140"`; F108'de 141). `gif_count="1"`: tek animasyon yuvası.

### 10.2 Yükleme blob'u

| Ofset | İçerik |
|---|---|
| 0 | kare sayısı F (1–140) |
| 1..F | kare başına gecikme, **2 ms birimi** (ms/2, 1–255 → 2–510 ms) |
| F+1..255 | `FF` |
| 256 + i×64800 | kare i (64.800 bayt) |
| son | 4096'nın katına kadar `FF` dolgu |

- Kaynaklar: S1 LcdAnimation.cs:26-33, 127-145; S2 encode.rs:103-111 (100 ms + 40 ms → `02 32 14`).
- Gecikme birimi [ÇELİŞKİLİ ama çözülmüş]: W1 "centisaniye/2" diyor. S11'e göre vendor `gif_centiseconds × 5` kullanıyor; bu ms/2 ile aynı şey. Birim **2 ms**.
- Tek karede gecikme baytı [ÇELİŞKİLİ]: S5/S10/S6 `00`, S7 `01` (resmi minimum 1). **Yakalama gerekiyor.**
- Sayfa sayısı: 1 kare = 16 sayfa; 24 kare = 380; **140 kare = 2215 sayfa = üst sınır** (S2 golden.json:1167-1172).

### 10.3 Aktarım

1. MI_02 input kuyruğunu boşalt.
2. `04 18` (ACK).
3. `04 72 SLOT 00 00 00 00 00 PAGES_LO PAGES_HI` (ACK).
4. Her sayfa için: MI_02'ye `WriteFile(00 + 4096 bayt)` (zaman aşımı 3 s), ardından `ReadFile` ile Input ack (sürücü 300 ms bekler). Ack = `01 5A 02 00 …` (S4:533; W1; W2).
5. `04 02`. **`04 F0` göndermeyin**: animasyon oynatıcısını yeniden başlatır ve kalıcılığı bozar (W2; S7 ak820.py:75).

- Kaynaklar: [AK980-HW: S1 SonixKeyboard.cs:368-387; S2 PROTOCOL.md:107-110, 2026-10-28] [KARDEŞ-HW: S4 tek kare test görüntüsü yön ve renk doğru, S6/S7 AK820].
- `04 18` ve `04 72` ile duyurmadan FF68'e yazmak takılmaya yol açar (S7 pantalla.md:30-33).
- Yarıda kesilen yükleme ekranı ve saati dondurur. Kurtarmak için saat paketi gönderin ya da `04 02` + `04 F0` deneyin (S7; S9 control.py:137-146).
- **SLOT** [ÇELİŞKİLİ]:
  - Stok değer `LCDViewList::GetCurSel()+1` = **1**.
  - AK980'de slot 2 gönderilince slot 1'deki animasyonun yerine geçti; tek animasyon tutuluyor (S2 PROTOCOL.md:112-114).
  - AKS075'te `03` yakalamayla doğrulanmış (W2). AK820'de `03` kısa süre görünüp kayboldu, `01` kalıcı oldu (S6 COMPATIBILITY.md:112-135).
  - **AK980 PRO için 1 kullanın.** Yakalamayla doğrulayın.
- **Kalıcılık:** yüklenen animasyon harici SPI NOR flash'a yazılır.
  - AK35i firmware'inde `0x72`, yazma adresini `0x950000` yapar; her 4096 baytlık blokta adres 0x1000 artar. Fabrika açılış animasyonu `0x340000`'da durur ve üzerine yazılmaz (S4:421-422).
  - AK980'de: "keyboard acknowledges every page with an input report after writing it to flash" (S2 PROTOCOL.md:109).
- **Geri okuma yok** (S2:110; S4:421).
- **Hız:**
  - AK980'de sayfa başına ~145 ms. Tek kare (16 sayfa) ≈ 2.3 s; 28 kare (444 sayfa) ≈ 64 s; 140 kare ≈ 5.4 dk (S2 PROTOCOL.md:108-116; S1 README.md:93).
  - AK35i'de blok başına 0.16 s (S4:533).

### 10.4 Saat senkronu (`04 28`)

`04 18` → `04 28 00 00 00 00 00 00 01 00` → veri (**ACK'li**) → `04 02` (F0 yok).

| P | Değer |
|---|---|
| 0 | 00 |
| 1 | SLOT / sayfa (stok: seçili liste öğesi + 1 = **01**) |
| 2 | `5A` |
| 3 | YY = yıl − 2000 |
| 4 | ay (1–12) |
| 5 | gün |
| 6 | saat (0–23) |
| 7 | dakika |
| 8 | saniye |
| 9 | 00 |
| 10 | haftanın günü (0 = Pazar; `SYSTEMTIME.wDayOfWeek`) |
| 11–61 | 00 |
| 62–63 | `AA 55` |

- Değerler ikili (binary), **BCD değil**. Yerel saat kullanılır.
- Golden örnek (S2 golden.json:1081): `00 01 5A 1A 0A 1D 0F 07 20 00 04 … AA 55` = 2026-10-29 15:07:32, Perşembe.
- Kaynaklar: [AK980-HW: S1 SonixKeyboard.cs:343-365; S2 2026-10-28] [AK980 PRO-HW: S3 Frida ile yakalanıp tekrar oynatıldı ve TFT saati değişti, ak980pro_vendor.md:227-261].
- [ÇELİŞKİLİ, çözüldü] Eski kaynaklarda P[10]=`04` sabit. Aslında yakalamanın yapıldığı gün Perşembe'ydi; doğrusu haftanın günü (S3; S8 proto.rs:357-362).
- **Her saat paketi ekranı saat sayfasına geçirir** (SLOT değerinden bağımsız) [AK980-HW: S2 PROTOCOL.md:91-94]. Bu yüzden S2 saati yalnızca haftalık gönderiyor. Stok uygulama her açılışta saat ve sayfa gönderiyor (S1 README.md:14).
- Klavye tak-çıkar sonrası 2 s beklenmeli: tüm arayüzlerin hazırlanması gerekiyor (S2 agent/mod.rs:484-510).
- Wired `0C 10` (dongle komutu) saati değiştirmedi (S3 donanım negatifi). O komut yalnızca dongle üzerinde çalışır (§11).
- [AK980 PRO-HW: bu proje, 2026-11-10] Veri paketinin yankısı gönderilenle bayt bayt aynı; `04 02` yankısı durum 01, bayt toplamı **0** (ışık akışlarından farklı). Ayrıntı: `docs/donanim-dogrulama.md`, "Saat senkronu".

### 10.5 Diğer ekran komutları

- **USB komutu yok:** sayfa değiştirme, ekranı açma/kapama, yerleşik animasyon seçme, dil, ekran parlaklığı için bilinen bir komut bulunmuyor.
  - Firmware kombinasyonları: Fn+Del ekran aç/kapa (AK980), Fn+knob → ekran menüsü.
  - AK35i'de Fn+PrtSc animasyon/ana sayfa, Fn+ScrLk ekran aç/kapa, Fn+C/V dil (S4 manual).
  - AK35i firmware'inde Fn+Q/W/E/R/T, ekran modunu 1/2/4/8/0x10 yapan eylem kodları üretiyor (S4:535).
  - "No known command switches to the animation page without uploading" (S2 PROTOCOL.md:114-115).
- **Canlı / akış ekran yolu (flash'sız): YOK** (bu firmware neslinde).
  - Her `04 72` harici flash'a yazar; 2.4G `7F 03` ekran yolu da `0x950000`'a yazar (S4:466).
  - S11'deki RAM'de kalan `AA 50 …` (SET_TFT_USER_ANIMATION) yolu yalnızca **farklı bir firmware neslinde** var: AK820 Pro fw 1.16, FF67 4104 baytlık arayüz. AK980 PRO'nun FF13/FF68 tanımlayıcısında böyle bir yol yok. Bu yolu ancak tanımlayıcı şekli değişirse (FF67 görünürse) değerlendirin.
  - Sonuç: dinamik içerik yalnızca (a) saat sayfasıyla (RTC'ye yazılıyor, büyük olasılıkla flash'a değil [ÇIKARIM]) ve (b) önceden yüklenmiş, döngüde oynayan bir animasyonla gösterilebilir.
  - Sık yükleme flash'ı aşındırır. Tipik NOR flash sektör başına ~10^5 silme döngüsü dayanır (genel bilgi; AK980 PRO'nun flash çipi doğrulanmadı; AK820'de PY25Q128HA, W5). Dakikada bir tek kare yükleme yaklaşık 70 günde bu sınıra ulaşır.
- **Sınır aşımı tehlikesi:** AULA F108 Pro'da 214 karelik bir yükleme (3386 sayfa), 141. kareden sonrasını ekran menüsü grafiklerinin üzerine yazdı. Kalıcı hasar oluştu; Fn+Esc ve ISP ile yeniden flash'lamak düzeltmedi (W1; S4:570). **F ≤ 140 ve PAGES ≤ 2215 kesin sınır.**

---

## 11. 2.4G dongle protokolü (`0C45:FEFE`, MI_03, FF60)

### 11.1 Çerçeve

33 baytlık Output: `[00][CMD][LEN][SEQ/ARG][≤28 veri][…00][SUM]`.

- SUM = önceki tüm baytların (report ID dahil, o da 0) toplamı mod 256; son baytta durur.
- Cevap ilk 3 baytı (CMD LEN SEQ) yankılar ve kendi toplamıyla biter. Bir istek iki özdeş cevap üretebilir.
- Okuma döngüsü: önce input kuyruğunu boşalt, yazma zaman aşımı 500 ms, ardından ≤20 okuma × 10–50 ms. İlk 3 baytı eşleşmeyen cevapları atla (S1 SonixDongle.cs:33-56; S2 device.rs:540-564; S4:429).
- Oturum yok: her paket hemen etkili olur. Toplam yanlışsa klavye cevap vermez (S4:449).
- Uzun süre boşta kaldıktan sonra ilk istek yalnızca radyo bağlantısını uyandırır; isteği iki kez gönderin (S1 :61-63). Gecikme: boşta sonrası ilk istek ~120 ms, sonrakiler ~13 ms (S2).
- Echo yalnızca dongle'ın paketi aldığını gösterir. Klavye uykudaysa ya da mod değiştiriyorsa uygulanmayabilir (S7).

### 11.2 Komutlar

| CMD | Paket | Etki | Flash? | Güven |
|---|---|---|---|---|
| `20 01` | `00 20 01 00 … 21` | pil: cevap `20 01 00 PCT` (PCT 0 = bilinmiyor; en fazla 100'e kırp). Stok sürücü 15 s'de bir yokluyor. | hayır | [AK980-HW: S1, S2 golden.json:1141] |
| `0C 10 00` | `0C 10 00 00 SLOT 5A YY MM DD hh mm ss 00 DOW 00 00 00 AA 55` (+sum). Golden: `00 0C 10 00 00 01 5A 1A 0A 1D 0F 07 20 00 04 00 00 00 AA 55 … F1` | saat | RTC | [AK980-HW: S2 2026-10-29, golden.json:1099] |
| `02 00` | probe | bilgi bloğu | hayır | [KARDEŞ-HW: S4, S8] |
| `05 10 00` | + wired `04 13` verisinin ilk 16 baytı (AA 55 p17–18). Örnek: `05 10 00 01 FF 00 00 00 00 00 00 00 04 03 00 00 00 AA 55` | ışık | **evet** (sayfa 0x9800) → rate-limit | [KARDEŞ-HW: S8, S7; S4 statik] |
| `07 10 00` | + `04 17` verisinin ilk 16 baytı | seçenekler | evet (0x9000) | [KARDEŞ-HW: S7 (test edilmedi), S4 statik] |
| `10` / `12` `1C SEQ` | 576 baytlık katman tablosu, 21 paket (20×28 + son paket uzunluk 0x10) | base / Fn keymap | evet (0x9400/0xAC00) | [KARDEŞ-HW: S7] |
| `03` / `0D` `04 IDX` | + 4 baytlık giriş | tek tuş base / Fn | evet (1 KB sayfa sil/yaz) | [KARDEŞ-HW: S8 yakalama `03 04 06 03 E9 00 00`] |
| `09 1C SEQ` | makro görüntüsü, 28 baytlık parçalar | makro | evet (0xA000) | [KARDEŞ-HW: S7] |
| `05 10 00 80…` ardından `14 1C SEQ`×21 | per-key tablo | per-key | evet (0x9C00) | [ÇIKARIM: S4 statik] |
| `0B 1C 00` | `FG BG BGBRIGHT` + sütun yükseklikleri (0–6; sürücü 21 gönderiyor, firmware 18 okuyor) | **müzik çubuk grafiği, yalnızca RAM** | **hayır** | [KARDEŞ-HW: S4 AK35i ~28 fps; S8 ~33/s] |
| `7F 03 00 SLOT CNT24` ardından `(80+(i>>16)) (i&FF) ((i>>8)&FF)` + 28 bayt | ekran | evet (0x950000); 2325 paket ~62 s | [KARDEŞ-HW: S4] |
| `1F` | keymap sıfırla | **engelle** | evet | [ÇIKARIM: S4 statik] |
| `0F` | factory restore | **engelle** | ? | [ÇIKARIM: S4 statik] |

- Kendiliğinden gelen raporlar: `55 FC 04` + 29 sıfır (toplam yok) bağlantı koptuğunda (S2 PROTOCOL.md:196-201). `05 A6 FF 01` / `05 A6 FE 02` (S4).
- 2.4G'de per-key **gerçek zamanlı** kare **yok**. Yalnızca `0B` sabit paletli çubuk grafik var (S4:466, 547).
- Resmi sürücü 2.4G'de yapılandırma yapmıyor (S2 PROTOCOL.md:35). AK980 PRO sürücüsünün 2.4G arayüzünde hangi özelliklerin göründüğü [AÇIK SORU].
- **Akış halinde gönderilen yazımlar (`09`, `10`, `12`, `14`, `80–8F`) tehlikelidir:** sıra numarasının üst sınırı yok. Fazla paket bir sonraki sayfayı siler: `09` paket 110 Fn katmanına taşar; 36'dan fazla `10` ışık sayfasını, 36'dan fazla `14` makro sayfasını siler (S4:473). SEQ 0'dan başlamalı, sırayla ve asla fazla gönderilmemeli.
- **Çerçeve uyumu:** sürücü toplamı 65 baytlık bir tamponda hesaplıyor ama 33 bayt yazıyor. Dongle firmware'i (CH58x) watchdog'u etkinleştirmiyor; radyo enumerated görünürken takılabiliyor. Kurtarmak için USB reset gerekiyor (S8 watchdog.rs).

---

## 12. Tehlikeli komutlar: ASLA gönderme (blocklist) ve allowlist önerisi

### 12.1 Mutlak blocklist

| Desen | Neden | Kaynak |
|---|---|---|
| FF13'e herhangi bir rapor; P[0..7] = `AA 42 89 5A FF 71 62 CC` | HFD firmware'i `0x209A`'ya atlar: güncelleme/bootloader modu | S4:443; W6 sonix-flasher |
| P[0..7] = `AA 55 A5 5A FF 00 33 CC` | eVision reboot-to-bootloader sihirli dizisi | W6 |
| `01/05/07 AA 55 00 …` | SN32 ISP komutları (init/prepare/reboot) | W6 |
| `04 38 …` | firmware ekran yazma yolu, **harici flash adres 0'dan** başlıyor (sürücü göndermiyor) | S4:421 |
| `04 72` ile F > 140 ya da PAGES > 2215 | menü grafiklerinin üzerine yazar (kalıcı) | W1; S4:570 |
| `04 72` sonrası `04 F0` | kalıcılığı bozar | W2; S7 |
| `04 50`, `04 51`, `04 33`, "response time" amaçlı `04 19` | doğrulanmamış veya yanlış (S5); `04 19` makro tablosunu siler | S5 hid.js:771-833 |
| Sürücüde görülmeyen her opcode: `04 05`, `10`, `12`, `14`, `16`, `22`, `26`, `AB`, `E0`, `F1`–`F4`, `F6` | firmware kabul ediyor ama işlevi doğrulanmamış | S4:408 |
| Başında `04` olmayan, duyurulmuş bir veri fazının dışında gönderilen FF13 paketi (örneğin `20 01` "kablolu pil") | işlevsiz ya da belirsiz | §3 |
| FF68'e SET_REPORT (`HidD_SetOutputReport`) | kardeş firmware'i çökertti | W1 |
| Duyurulmuş `04 72` olmadan FF68'e yazmak | takılma | S7 |
| Bootloader/ISP cihazlarını açmak: `0C45:7140`, `7010`, `7040`, `7900` | MCU flash'lama | W5; W6 |
| BLE GATT `FEE0/FEE1` yazımı | WCH CH58x OTA (`80` program, `81` erase, `82` verify, `83` end, `84` info) | S4:553-555 |
| Dongle `0F` (factory restore), `1F` (keymap reset) | geri dönüşsüz ayar silme | S4 |
| Dongle akış yazımlarında fazla SEQ | komşu flash sayfalarını siler | S4:473 |
| `FirmwareUpdateTool.exe` / `.SN8` / ISP opcode'ları `0x20–0x2A` (updater çerçevesi) | firmware güncelleme | W1 firmware-update.md; S3 :635-660 |
| Arka arkaya `04 13`, `04 23`, `05 10`, `03` gönderimi (slider/akış) | iç flash aşınması | S4:473; S8 |

### 12.2 Allowlist (Rust core'da "yalnızca bunlar" kuralı)

**FF13 başlık opcode'ları:** `18`, `02`, `F0`, `13`, `23`, `F5`, `17`, `11`, `27`, `19`, `15`, `28`, `72`, `20`.

- Her biri yalnızca §2.3'teki akış şablonu içinde ve doğru sayaçla gönderilmeli.
- Veri paketleri yalnızca duyurulan sayaç kadar gönderilmeli.
- Değer doğrulaması:

| Alan | İzin verilen aralık |
|---|---|
| mod | {0x00–0x13, 0x80} (yan ışık 0x20–0x29 yalnızca deneysel bayrakla) |
| parlaklık / hız | 0–5 |
| yön | 0–3 |
| uyku | 0–3 |
| tepki | 1–5 |
| keymap idx | 1–142 |
| makro boyutu | ≤ 0xE00 ve bütçe ≤ 0xA8C |
| ekran | F ≤ 140, PAGES ≤ 2215, SLOT = 1 |

**Dongle CMD:** yalnızca `20 01` ve `0C 10`. `0B` ve `02 00` için ayrı bir onay katmanı olmalı.

---

## 13. Çelişkiler özeti

| # | Konu | Varyantlar | Önerilen karar |
|---|---|---|---|
| C1 | Işık kuyruğu | `AA 55`@14–15 (S1, S2, S8 yakalama, W1 yakalama) / `55 AA`@14–15 (S6, S7, S9, S10) / `AA 55`@62–63 (S5) | `AA 55`@14–15 |
| C2 | Mod numaraları | 0=Off,1=Static (çoğunluk) / 0=Static…0x13=Off (S3) | çoğunluk |
| C3 | Static/Off | 01/00 (S1, S2, S8) / 07 hız 0 ve 02 parlaklık 0 (S6, S7, S9) | 01/00 |
| C4 | Yön | L/D/U/R (S1, S2, S7, S9, S10) / L/U/D/R (S5) / {0,1} ve {0,2,3} (S4) | yakala |
| C5 | START P[8] | 0 (S1, S2, S8, resmi) / 1 (S9, S10, S7 bazı akışlar) | 0 (sayaç) |
| C6 | Işık kapanışı | 02+F0 (S1, S2, S4, S8) / yalnızca F0 (S5, S9, S7) / yalnızca 02 (S10) | 02+F0 |
| C7 | `04 02` durum | 01 + toplam (S6, S7, W1) / FF (S2) | yakala |
| C8 | Seçenek baytları | [2]AltTab [4]Win (S1, S2, S4) / [2]Win [4]AltTab (S7) | yakala |
| C9 | Ekran SLOT | 1 (S1, S2, S6, S7, S8, resmi) / 3 (S5, W2 AKS075) / 2 (gohv) | 1 |
| C10 | Tek kare gecikmesi | 00 (S5, S6, S10) / 01 (S7) | yakala |
| C11 | Gerçek zamanlı kayıt düzeni | konumlu idx×4 (S2) / ardışık (S4, S7, S8, W3) | konumlu |
| C12 | Akış durunca | son kare kalır (S2) / hemen geri döner (S4, S7) / 2 s keepalive (W3) | test et |
| C13 | Yan ışık numarası | 0x20–0x29 (S1, S2) / 0x1F–0x28 (W1) | yakala |
| C14 | Dongle PID | FEFE = dongle (AK980: S1, S2, S3) / FEFE = BT (AK35i, AK820) | bus türüyle ayır |
| C15 | Kablolu pil | `20 01` → FF "dolu" (S3, S6, S9) / kablolu pil yok (S2, S4) | kablolu pil yok |
| C16 | F5 sayacı | P[8]=0 (S1) / 09 (S4, S8) | ikisi de cevap veriyor; yakala |
| C17 | Komut gecikmesi | 35 (resmi XML) / 40 (S5, S10) / 30 (S3) / 10 (S11 açıklaması) | 35 |
| C18 | S3 opcode tablosu | wired ve 2.4G opcode'ları karışık (`07 10`, `09 1C`, `0B 1C`, `20 01`, `7F 03` aslında dongle komutları); report ID'yi `04` sanan kayma | S3 tablosunu yalnızca dongle ipucu olarak kullanın |

---

## 14. Açık sorular / doğrulanacaklar (resmi AK980 PRO sürücüsünün USB yakalamasıyla)

**Yöntem:** USBPcap + Wireshark, ya da API Monitor/Frida ile `hid.dll!HidD_SetFeature/HidD_GetFeature`, `kernel32!WriteFile/ReadFile/DeviceIoControl` (0xB0191/0xB0192) çağrılarını hook'layın. S3 bu yöntemle AK980 PRO'da saat paketini doğruladı. Her UI eylemini tek tek, aralarında bekleyerek kaydedin. Ham pcap'te tuş vuruşları bulunur; paylaşmadan önce temizleyin.

1. **`04 02` cevabı:** her akıştan sonra P[3] (01 mi FF mi?) ve P[4..5] (veri toplamı mı?) (C7).
2. **Işık:** 20 modun her biri; mod 10/11/12/16/18 için her yön (C4); rainbow açık/kapalı; parlaklık 0; "Off". Ayrıca `04 F0`'ın okunup okunmadığı.
3. **Seçenekler:** Alt+Tab, Alt+F4 ve Win kutularını **tek tek** işaretleyip her birini yakalayın (C8); uyku 4 değer; tepki 1–5.
4. **Yan ışık:** AK980 PRO sürücüsünün UI'ında yan ışık menüsü var mı (`layouts/rgb-keyboard.xml` `sidelight=` özniteliği)? Varsa mod baytları (C13) ve fiziksel "side light strip"in tepkisi.
5. **Gerçek zamanlı ışık ve müzik ritmi sayfaları:**
   - kayıt düzeni (C11), paketler arası süre, keepalive;
   - **akış durdurulunca sürücü ne gönderiyor** ve klavye kaç ms sonra kayıtlı efekte dönüyor (C12);
   - sayfadan çıkınca bir `04 13` gönderiliyor mu?
6. **TFT yükleme:**
   - SLOT baytı (C9);
   - tek kare ve çok kare başlığı (C10, gecikme birimi);
   - sayfa ack baytları;
   - `04 02`'den sonra başka paket var mı;
   - toplam süre.
   - (140'ı aşan kare sayısını **asla** test etmeyin.)
7. **Saat:** P[1] değeri; sürücü açılışta otomatik saat ve sayfa gönderiyor mu; "Time Syns" düğmesi.
8. **F5 okuma:** P[8] değeri (C16), okuma sayısı, ilk okumanın echo olup olmadığı; sürücü hangi sayfada F5 yokluyor.
9. **Keymap:**
   - Stok Fn katmanında varsayılan sıfır olmayan girişler var mı?
   - "Reset keyboard" ve "Factory reset" düğmeleri hangi paketleri gönderiyor (yeni bir opcode var mı)?
   - "Open program/website/text" Input raporlarının biçimi.
10. **Makrolar:** AK980 PRO'da bütçe (0xA8C) ve 0xE00 sınırı; `04 19` öncesinde `04 18` var mı.
11. **2.4G:**
    - AK980 PRO sürücüsü 2.4G modunda hangi özellikleri sunuyor ve hangi komutları gönderiyor (`05 10`, `07 10`, `0B`, `7F 03`)?
    - Pil yoklama aralığı.
12. **Bluetooth:**
    - BT HID aygıtının PnP kimliği: dongle ile aynı `FEFE` mi (C14)?
    - Windows'ta GATT Battery Service 0x180F okunabiliyor mu (yalnızca okuma)?
13. **Knob:** MI_01 rapor tanımlayıcısı (consumer volume mi, Dial mı?); Fn+knob basmanın bir HID raporu var mı; knob'un keymap'te indeksi var mı.
14. **Oyun ışığı Fn+1/2/3** ve diğer Fn kombinasyonları: AK980 PRO kılavuzu ve sürücü `1033.lan` metinleri.
15. **Firmware sürümü:** kullanıcının kartındaki `bcdDevice`; sürücünün "Firmware Update" sayfasının gösterdiği sürüm.
16. **Logo/yan şerit LED'leri:** kullanılmayan indekslerle `04 20` testi. S2'de etkisi olmadı; AK980 PRO'da da tekrarlanmalı.
17. **1033.lan 500–519 "kozmik" efekt adları:** ne işe yarıyorlar (profil ön ayarı mı, ayrı modlar mı)?
18. **`04 20` flash davranışı:** uzun bir akış (örneğin 1 saat) sonrasında kayıtlı ayarların bozulmadığını ve `04 02` toplam alanının davranışını gözlemleyin.
19. **(Opsiyonel, risk değerlendirmesinden sonra) statik okuma opcode'ları** `04 10/26/12/16/22`: sürücüde yoklar; yalnızca uzman kararı ve yedekle denenmeli.
20. **Windows'ta `HidD_GetFeature` zamanlaması:** 0 ms aralıkla sessiz no-op olduğu biliniyor (S3). Minimum güvenli aralığı (35 ms altı) ölçün; bu, toplu yazımları hızlandırabilir.

---

## Kaynaklar

Referans repolar geçici bir klasöre `git clone --depth 1` ile klonlandı; yalnızca okundu, hiçbir kodu çalıştırılmadı.

| ID | Proje (commit, tarih) | Hedef cihaz / kanıt türü | Okunan başlıca dosyalar |
|---|---|---|---|
| **S1** | github.com/donglover69/sonix-keyboard-studio (f66bb72, 2026-10-26) | AK980 V2 `0C45:8009` + AULA F108 Pro; stok DeviceDriver.exe (AK980 build) Ghidra/capstone + donanım | `README.md:7-102`, `agents.md:21-30`, `src/Sonix.Keyboard/SonixKeyboard.cs:26-387`, `Lighting.cs:3-124`, `KeyboardModel.cs:43-99`, `SonixDongle.cs:5-65`, `KeyMap.cs:8-415`, `Macro.cs:125-139`, `Lcd/LcdAnimation.cs:26-243`, `Layout/ak980-layout.json` |
| **S2** | github.com/YellowHearth1/ajazz-ak980-v2-linux (6963448, 2026-11-05) | "AK980 V2" = USB "SONiX AK980 PRO" 8009/FEFE; donanım doğrulama tablosu + golden baytlar | `docs/PROTOCOL.md:14-218`, `README.md:234-389`, `crates/ak980-core/src/device.rs:28-610`, `hid.rs:189-375`, `lighting.rs:10-189`, `keymap.rs:25-637`, `macros.rs:16-267`, `lcd/encode.rs`, `engine.rs:25-559`, `tests/golden.json`, `data/layout.json`, `CONTRIBUTING.md:47-53` |
| **S3** | github.com/Aiacos/ajazz-control-center (03a29f7, 2026-07-03) | AK980 PRO driver Beta 1.0.0.6 statik (Ghidra) + Frida ile AK980 PRO'da saat/FF13 doğrulaması; statik kısım güvenilmez (C18) | `docs/protocols/keyboard/ak980pro_vendor.md:1-1177` (özellikle 37-89, 227-261, 341-475, 576-660), `ak980pro_tft_protocol.md:8-14`, `docs/research/reverse-engineering/ak980-keyboard.md:30-213`, `microdia_dongle.md` |
| **S4** | github.com/Ike-li/ak35i-v3-max-macos-control-center (1a4ccec, 2026-10-16) | AK35I V3 MAX `0C45:8009`; driver ve firmware V1.17 statik + macOS donanım | `docs/STATIC_REVERSING.md:17-587` (52-59, 73-74, 95-152, 171-288, 405-477, 483-573), `docs/PROTOCOL_STATUS.md:7-39`, `docs/SAFETY_MODEL.md:7-11`, `Sources/AK35iClockSync/*.swift` |
| **S5** | github.com/SlightlyLookAround/ak980_web (703e918, 2026-09-12) | AK980 V2 WebHID; çoğu doğrulanmamış | `js/hid.js:4-833`, `js/keyboard.js:2-196`, `bridge/server.py`, `README.md` |
| **S6** | github.com/wsclx/ak820pro-modder (55a2ca2, 2026-10-25) | AK820 `0C45:800A` FF13; donanım (vespovios/adamauthor) | `crates/ak820-protocol/src/legacy.rs:1-438`, `legacy_rgb.rs`, `legacy_tft.rs`, `docs/COMPATIBILITY.md:21-135`, `docs/MACOS-800A.md:10-167`, `docs/PER-KEY-RGB.md:16-84`, `docs/PROTOCOL.md`, `docs/HANDOFF.md` |
| **S7** | github.com/gha-projects/ak820-mac (abd46c4, 2026-10-17) | AK820 800A; driver v1.0.0.5 disasm + donanım | `docs/PROTOCOL.md:13-201`, `docs/pantalla.md`, `docs/receptor-24g.md`, `ak820.py` |
| **S8** | github.com/DarkTiengo/ak820ctl (2903211, 2026-10-24) | AK820 Pro 800A fw 1.10 + dongle FDFD; USBPcap yakalamaları | `src/proto.rs:7-376`, `src/device.rs:11-261`, `src/ambient.rs`, `src/watchdog.rs`, `README.md:15-222` |
| **S9** | github.com/gusleig/ajazz-ak820-pro-mac (2ad5c55, 2026-06-30) | AK820 8009 macOS | `ak820/control.py:32-187`, `docs/protocol.md:5-555`, `docs/REMAP-RESEARCH.md`, `docs/DISCOVERY-LOG.md` |
| **S10** | github.com/kabelklaus/ajazz-ak820-pro (4b69f3c, 2026-08-11) | AK820 WebHID 8009/800A | `app.js:194-1267`, `README.md:94-205` |
| **S11** | github.com/fromagge/ak820-screen (9030b81, 2026-10-21) | AK820 Pro 800A fw 1.16, "AA" protokolü (farklı nesil) | `PROTOCOL.md:11-139`, `captures/vendor-upload.log` |
| **S12** | github.com/fraustiz/freejazz (bd2d449, 2026-10-30) | AK980 MAX `0C45:8043`, farklı "AA" protokol ailesi (karşılaştırma için) | `docs/protocol.md`, `crates/freejazz-protocol/src/*.rs` |
| W1 | github.com/parsiya/f108-pro (`ai-docs/hid-protocol.md`, `lcd-upload-investigation.md`, `firmware-update.md`), parsiya.net/blog/ai-borked-keyboard/ | AULA F108 Pro 800A; Ghidra + Wireshark, kısmi donanım; 214 kare hasarı | |
| W2 | github.com/aar-rafi/aks075-linux (`docs/protocol.md`) | AJAZZ AKS075 8009/FEFE; Wireshark yakalaması | |
| W3 | github.com/hcode10/OpenRGB-With-AulaF108Pro-support (`Controllers/AulaF108Pro/AulaF108ProController.cpp`) ve gitlab.com/CalcProgrammer1/OpenRGB/-/issues/5253 | F108 Pro direct mode `04 20` + 2 s keepalive | |
| W4 | github.com/KyleBoyer/TFTTimeSync-node (`src/packets.ts`, `src/light-modes.ts`) | AK35i V3 MAX saat ve ışık | |
| W5 | github.com/fpb/ajazz-ak820-pro | AK820 Pro teardown: HFD80CP100, CH582F, PY25Q128HA, ISP `0C45:7140` | |
| W6 | github.com/SonixQMK/sonix-flasher (`src/main/python/main.py`) | bootloader sihirli dizileri, SN32 ISP | |
| W7 | github.com/not-nullptr/openajazz | AK35I/F75 Max ışık ve saat (saat kodunda hata var) | |
| W8 | github.com/gohv/EPOMAKER-Ajazz-AK820-Pro | AK820 Pro Rust CLI (S3 ve S10 üzerinden dolaylı) | |
| W9 | signalrgb.com/devices, forum.signalrgb.com/t/ajazz-ak820-pro/9969 | SignalRGB "Sonix" eklentisi AK980 Pro'yu destekliyor; kaynak kodu açık değil | |

Bulunamayanlar (negatif sonuçlar):

- Açık AK980 PRO sürücü XML/lan dökümü yok; S3'ün alıntıları dışında bir şey bulunamadı.
- OpenRGB'de birleştirilmiş (merged) bir SONiX 0C45 kontrolcüsü yok.
- SignalRGB eklentisinin kaynak kodu yok.
- AK980/AK980 PRO için brick raporu yok.
- CH58x dongle OTA aracı/protokolü yok.
- AK980 PRO kılavuzunun tam Fn tablosu bulunamadı (manuals.plus erişimi engellendi).

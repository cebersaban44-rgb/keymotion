# Klavye Stüdyo — Proje durumu ve devir notu

Son güncelleme: **2026-11-10**. Bu belge, bağlam sıfırlandığında ya da yeni biri projeye katıldığında okunacak **ilk dosyadır**. Ayrıntılar bağlantılı belgelerde.

| Belge | İçerik |
|---|---|
| `docs/DURUM.md` (bu dosya) | Neredeyiz, ne yaptık, neden, sırada ne var, dersler |
| `docs/donanim-dogrulama.md` | **Gerçek cihazda ölçülen** her şey (en güvenilir kaynak) |
| `docs/research/protokol-ff13.md` | Topluluk kaynaklarından derlenen bayt düzeyinde protokol. Ölçümle çelişen yerlerde donanım günlüğü geçerlidir. |
| `docs/research/ekran-ve-veri-kaynaklari.md` | Ekran fizibilitesi, flash ömrü hesabı, Windows veri kaynakları |
| `docs/mimari.md` | ADR-001: neden Tauri 2 + Rust, katmanlar |
| `docs/teknik-analiz.md` | İlk keşif: model, BT servisleri, Fn kombinasyonları |
| `docs/gelistirme-akisi.md` | Dallar (`main` / `develop`), PR'lar, birleştirme yöntemi, sürüm ve hotfix adımları |

---

## 1. Hedef

AJAZZ AK980 PRO klavyesinin (sonra benzer modellerin) **her özelliğini** kontrol eden, ürün kalitesinde, açık kaynak olacak bir Windows uygulaması. Kapsam:
- **Işık:** efektler, tuş başına renk, canlı efektler
- **Tuşlar:** tuş atama, makrolar, Fn katmanı
- **Ekran:** 240×135 TFT
- **Bağlantı ve donanım:** bağlantı modları, pil, seçenekler, topuz

Çalışma ilkeleri (kullanıcıyla kararlaştırıldı):
- Topluluk projeleri **kopyalanmaz**. Yalnızca referans alınır, kaynak gösterilir ve her bilgi cihazda doğrulanır.
- Önce Windows ve bu model. Mimari çoklu modele hazır kurulur.
- Kalite çıtası yüksek (AA+). Güzel ve erişilebilir arayüz, TR/EN.
- `main` yayındaki sürümdür, geliştirme `develop`'ta yapılır. Her iş ayrı dalda yapılır ve PR ile birleşir; `main`'e doğrudan commit atılmaz. Ayrıntı: `docs/gelistirme-akisi.md`.

## 2. Şu an çalışan özellikler (cihazda doğrulandı)

| Özellik | Durum |
|---|---|
| Cihaz tespiti (USB, 2.4G, BT), takma/çıkarma izleme (2 sn yoklama) | ✅ |
| BT pil seviyesi (GATT 0x180F) | ✅ |
| Genel Bakış sayfası: ölçülmüş 97 tuşlu düzen, ekran taklidi, mod anahtarı, pil | ✅ |
| Işıklar → **Efektler**: 20 firmware efekti, renk, gökkuşağı, parlaklık, hız, yön (ölçülmüş, hareketiyle adlandırılmış iki yön); "Klavyeye uygula" (flash) | ✅ Kullanıcı onayladı; yönleri klavyede de denedi. |
| Işıklar → **Ayna**: klavyenin gerçek renklerini saniyede yaklaşık 4 kez `04 F5` ile okuyup gösterir | ✅ |
| Işıklar → **Tuş başına renk**: Fırça ve Seç araçları, hazır desenler, canlı önizleme (RAM), kalıcı kayıt, kayıt durumu ve "Kayıtlıya dön" | ✅ Eksik tuş hatası düzeltildi, akış uygulamada uçtan uca doğrulandı. Kullanıcının son denemesi bekleniyor. |
| Ayarlar: tema (sistem/koyu/açık), dil (TR/EN), Mica | ✅ |
| **Kaydedilmemiş değişiklik koruması**: kapatırken "Kaydet ve çık / Kaydetmeden çık / Vazgeç", kenar çubuğunda nokta | ✅ Uygulamada dört yol da denendi |
| Ayarlar → **Renk kalibrasyonu**: gamma (bağlı ya da kanal başına), beyaz dengesi, eğri grafiği, klavyede canlı test deseni (beyaz, gri basamaklar, palet), kalıcı ayar | ✅ Kullanıcı gri basamakları ve beyazı onayladı. |
| Genel Bakış → **Klavye saati**: bağlanınca otomatik eşitleme (günde en fazla bir kez, kapatılabilir), "Şimdi eşitle", son eşitleme zamanı | ✅ Uygulamada uçtan uca denendi; kullanıcı ekrandaki saatin doğru olduğunu onayladı. |
| **Tuş Atama**: temel ve Fn katmanı; tuş, kısayol (Ctrl/Shift/Alt/Win + tuş), medya, fare, tekerlek, devre dışı; taslak, kaydet, geri al, katmanı fabrika ayarına döndür; sayfada tuş testi kutusu | ✅ CLI ve uygulamayla klavyeye yazıldı; kullanıcı tuşlara basarak doğruladı (madde 19). |
| **Makrolar**: liste, tuşlara basarak kayıt (konuma göre), adım düzenleyici (bekleme, sıralama, fare tıklaması), bellek göstergesi, kaydet ve geri al; Tuş Atama'da "Makro" seçici ve oynatma biçimi | ✅ Uygulamadan klavyeye yazıldı; kullanıcı basarak doğruladı (madde 20, 21). |
| **Ekran Stüdyosu**: dosyadan (resim, GIF, video; kırpma, süre), yazı kartları (5 şablon), slayt; önizleme, onay, kütüphane, fabrika animasyonuna dönüş | ✅ Kullanıcı klavyede hepsini denedi (maddeler 23–25). |
| Ekran → **Işıkları uydur**: görselin paletinden tuş tasarımı (degrade, tek renk, görsel), Işıklar'da taslak | ✅ Kullanıcı mor kartta klavyede doğruladı (madde 26). |
| Ekran → **Canlı ekran**: canlı mod, günlük yükleme bütçesi, "Çalan şarkı" kartı, kapatınca önceki görseli geri yükleme | ✅ Kullanıcı klavyede denedi (madde 27). |
| Araçlar → **Odak zamanlayıcısı** (Pomodoro tekniği): odak ve mola süreleri; bütün klavye fazın rengiyle dolu başlar ve süre azaldıkça seçilen yönde boşalır (flash'sız; yön, renkler, parlaklık, nabız, faz geçişi, ses seçilebilir; "Klavyede dene"); faz başında ekran kartı; kenar çubuğunda kalan süre | ⏳ Zamanlayıcı ve ışık katmanı gerçek uygulamada ve klavyede denendi (CDP, günlük). Kullanıcının gözle denemesi ve ekran kartının klavyede yüklenmesi bekliyor (madde 28). |

## 3. Kronoloji: ne yaptık, neden

1. **Keşif.**
   - Klavye ilk başta Bluetooth'taydı. BLE'de üreticiye özel kanal yok, `FEE0` ise OTA (güncelleme) kanalı.
   - Sonuç: ayar yalnızca USB ve 2.4G'de mümkün. Bkz. `teknik-analiz.md`.
2. **Mimari kararı.**
   - Kullanıcı saf Rust istedi.
   - Tauri 2 (tek exe, Node yalnızca derleme zamanında) + "Rust öncelikli" kurallar önerildi ve kabul edildi.
   - Gerekçe: arayüz ekosistemi; ileride WebHID ile web sürümü. Bkz. `mimari.md`.
3. **Araştırma.**
   - İki alt ajan protokolü ve ekran fizibilitesini çıkardı.
   - En önemli kaynaklar `sonix-keyboard-studio` (S1) ve `ajazz-ak980-v2-linux` (S2). Bu ikisi bu klavyeyle aynı donanımı test ediyor.
4. **Çekirdek.** Cargo workspace ve crate'ler (§4), CLI (`kb`), cihaz tespiti, BLE pil.
5. **Arayüz kabuğu.** Tasarım dili "eloksal alüminyum / gösterge paneli" ve tek vurgu rengi "sinyal turuncusu".
   - Klavye görselinde güney yönlü LED parıltısı var.
   - Mod anahtarı gerçek klavyedeki sürgü anahtarının birebir çizimi.
6. **USB doğrulama.**
   - Arayüzler doğrulandı: `0C45:8009`, FF13 komut, FF68 ekran.
   - `04 F5` ile renk okuma ve `04 20` ile canlı ışık (RAM) doğrulandı.
   - Geometri kullanıcının fotoğrafından ölçüldü.
7. **Işıklar.**
   - Efekt (`04 13`) ve tuş başına renk (`04 23`) yazma doğrulandı.
   - `04 02` yankısında sağlama toplamı bulundu.
   - Tek iş parçacıklı cihaz aktörü kuruldu. Işıklar sayfası yazıldı.
8. **Uygulama içi hatalar.** Kullanıcı üç akışın da çalışmadığını bildirdi.
   - Kök neden: `04 F5` sayacı `00`.
   - Yan nedenler: IPC sırası, GET_FEATURE stall. Ayrıntı §6.
9. **Renk doğruluğu.** Palet renkleri soluk çıkıyordu. Neden gamma; çözüm `LedCalibration` (gamma 2.2).
10. **Kalibrasyon arayüzü.**
    - Kalibrasyon artık Rust tarafında kalıcı: `%APPDATA%\dev.klavye.studio\calibration.json` (sürümlü; bozuk ya da aralık dışı değerler güvenli varsayılana döner).
    - Ayarlar sayfasında düzenleniyor. Kaydırıcılar canlı karede anında etki eder; değer 400 ms durunca kaydedilir.
    - Ayna ters kalibrasyonla gösteriyor. Böylece tuş başına modda boyanan renkler aynen geri okunur.
    - Canlı önizlemede parlaklık artık gamma'dan **sonra**, doğrusal uygulanıyor. Önceden sRGB'de ölçekleniyordu; 3/5 parlaklık, kayıtlı tasarımdan çok daha koyu görünüyordu (`0.6^2.2 ≈ 0.32`).
    - IPC log'ları `debug` düzeyine çekildi. Proje için `rustfmt.toml` eklendi (120 sütun).
11. **Tuş başına renk akışı.**
    - **Kullanıcının bildirdiği sorunlar:**
      - "Temizle" klavyedeki renkleri silmiyordu.
      - Tıklanan tuş seçilen rengi almıyordu.
      - Eski renkler klavyede kalıyordu.
    - **Kök neden:** Firmware kare ve tablo girişlerini idx baytına göre uyguluyor. Boş yuva (`00 00 00 00`) hiçbir tuşu değiştirmiyor; uygulama ise yalnızca boyanmış tuşları gönderiyordu. `kb live-probe` ile ölçüldü.
    - **Düzeltme:** Her kare ve tablo bütün LED'leri açıkça tanımlıyor; verilmeyenler siyah.
    - **Ayrıca:**
      - Fırça rengi boyama anında okunuyor (React yeniden çizmeden tıklanınca eski renk geliyordu).
      - Kayıt durumu gösteriliyor ("Kaydedilmemiş değişiklik var" / "Son kaydettiğin tasarımla aynı") ve "Kayıtlıya dön" eklendi.
      - Canlı önizlemenin kaydetmediği açıkça yazıyor.
      - Boyanan tuşun rengi kenarında belirgin gösteriliyor.
12. **Fırça ve Seç araçları.**
    - Kullanıcı bir tuşa tıklayıp renkleri denedikçe tuşun değişmesini bekledi (seçim modeli). Fırça modeli korundu; kullanıcı basılı tutup boyamayı sevdi.
    - Renklerin üstüne açık bir araç seçici eklendi; altındaki açıklama aracın ne yaptığını anlatıyor.
      - **Fırça:** Renk seçilir, tuşlara tıklanır ya da basılı tutup gezdirilir.
      - **Seç:** Tuşlar tıklanarak ya da gezdirilerek seçilir (Ctrl/Shift ile ekle/çıkar). Renk seçimi seçili tuşları anında boyar ve seçim kalır. Esc seçimi bırakır, Delete söndürür, Ctrl+A tümünü seçer. Boş alana tıklamak da seçimi bırakır.
    - Tek tuş seçilince renk seçici o tuşun rengini gösterir.
    - Araç kalıcıdır (`ks.lighting.v1`); seçim kalıcı değildir.
13. **Kaydedilmemiş değişiklik koruması.**
    - **İstek:** Kullanıcı kaydetmeden kapattığında değişikliklerin sessizce kaybolmamasını istedi; bunun bütün bölümlerde olmasını önerdi.
    - **Rust (`lib.rs`):** `CloseRequested` olayını yakalar.
      - Arayüz `set_unsaved` ile "kaydedilmemiş bir şey var" demişse kapanmayı durdurur ve `close-requested` olayını yollar.
      - Arayüz 2 sn içinde `ack_close_request` ile cevap vermezse pencereyi yine kapatır; çökmüş bir webview uygulamayı kilitleyemez.
      - `close_window` (`destroy`) seçimden sonra soru sormadan kapatır. Ek pencere yetkisi gerekmez.
    - **Arayüz:**
      - `lib/unsaved.ts` ortak listedir. Her bölüm bir kanca ekler ve şunları verir: alan, ayrıntı, `save`, `discard`, `needsUsb`.
      - `UnsavedDialog` kapatırken soru sorar. "Kaydet ve çık" her öğeyi kaydeder, hata olursa pencere açık kalır. "Kaydetmeden çık" düzenlemeleri klavyedeki son kayda döndürür. "Vazgeç" ve Esc pencereyi kapatır.
      - Kenar çubuğu bekleyen bölümü sarı noktayla işaretler.
    - **Işıklar:**
      - `draft` alanı en son düzenlenen kurulumu (efekt ya da tuş başına) tutar; klavye aynı anda tek ışık kurulumu taşır.
      - Düzenleme klavyedeki kayıtla aynı hale gelirse bekleyen sayılmaz.
      - `draft`'tan önceki kayıtlı durumdan bekleyen düzenleme bir kerelik çıkarılır.
      - Kaydetme işlemleri store'a taşındı (`applyEffectDraft`, `savePerKeyDraft`, `discardDraft`).
    - **Uygulama içi sayfa geçişi soru sormaz:** Taslak saklanıyor, nokta hatırlatıyor.
    - **Sonraki adım:** Tuş atama, makro ve ekran bölümleri gelince `useUnsaved` içine kendi kancalarını ekleyecek.
14. **Dal düzeni, PR akışı ve CI.** Kullanıcıyla kararlaştırıldı; kurallar `docs/gelistirme-akisi.md`'de.
    - **Dallar:** `main` yayındaki sürüm, `develop` geliştirme. Her iş `feature/`, `fix/`, `docs/` ya da `chore/` dalında yapılıp PR ile `develop`'a girer (squash). Sürüm `develop` → `main` PR'ıyla (merge commit) ve `v0.x.y` etiketiyle, kullanıcının onayıyla çıkar.
    - **GitHub ayarları:**
      - Varsayılan dal `develop`: dışarıdan gelen PR'lar güncel koda gider, "Fixes #N" çalışır. Kullanıcılar uygulamayı Releases'tan indirir.
      - İki dal korumalı: PR zorunlu, force-push ve silme yasak, "Build and test" kontrolü birleştirme şartı. `main`'de kurallar yöneticiler için de geçerli.
    - **CI (`.github/workflows/ci.yml`):** Windows'ta çalışır. Adımlar: arayüz kurulumu, typecheck ve derleme; fmt, clippy, testler; üretilen TS tiplerinin güncel olduğunun kontrolü. İlk çalışma 5 dk sürdü.
    - **`.gitattributes`:** Bütün metin dosyaları LF; Windows'ta ts-rs ve CI aynı baytları görür.
    - **PR'lar:**
      - [#1](https://github.com/cebersaban44-rgb/klavye-studio/pull/1): ışık işleri (madde 10–13)
      - [#2](https://github.com/cebersaban44-rgb/klavye-studio/pull/2) ve [#3](https://github.com/cebersaban44-rgb/klavye-studio/pull/3): dal rehberi ve varsayılan dal
      - [#4](https://github.com/cebersaban44-rgb/klavye-studio/pull/4): CI
15. **Efekt rengi, parlaklık ölçeği ve ayna kırmızısı.** P1'deki "efekt rengine gamma gerekiyor mu?" sorusu göz yerine ölçümle cevaplandı. Ayrıntı: `donanim-dogrulama.md`, "Efekt rengi, parlaklık ölçeği ve F5'in kırmızı baytı".
    - **Efekt rengi:** Firmware gamma uygulamıyor. Uygulamanın kalibrasyonu artık efekt rengine de uygulanıyor.
    - **Parlaklık:** Firmware ölçeği ölçüldü: `v·m[n] >> 10`, `m = [0, 191, 382, 573, 764, 1016]`. Efektler ve tuş başına tablo aynı ölçeği kullanıyor. Canlı önizleme `n/5` yerine bunu kullanıyor.
    - **Ayna:** `04 F5` kırmızıyı `2·r mod 256` bildiriyor. Ayna artık kırmızıyı uygulamanın kayıtlı durumuna göre çözüyor (`kb-core/src/readback.rs`). Model dosyasına firmware göstergeleri (Num Lock, Fn) eklendi.
    - **Yan hata:** Tuş başına sekmesi açıldıktan sonraki 40 ms içinde başka sekmeye geçilirse bekleyen canlı kare akışı yeniden başlatıyordu ve akış bir daha durmuyordu. Kapanışta zamanlayıcı artık temizleniyor.
    - **Tuş başına ölçek:** İlk denemede CLI ile art arda flash yazma izin sınıflandırıcısına takıldı. Kullanıcının isteğiyle ayrıca ölçüldü: 5 seviyede 6 renk, 40 değerin hepsi tabloya uyuyor.
    - **Kalibrasyon metinleri:** Ayarlar'daki not, efekt rengine uygulanmadığını söylüyordu; düzeltildi. "Gri basamaklar" ipucu artık soldaki koyu basamakların klavyede de soluk görünmesinin normal olduğunu söylüyor.
16. **Otomatik birleştirme** (kullanıcının isteği). `develop`'a giden her PR'da auto-merge (squash) ve auto-fix açık; repoda `allow_auto_merge` ve `delete_branch_on_merge` açık. Şimdilik ikinci göz yok; yayından sonra dış katkılar için denetim kurulacak (P3).
17. **Saat senkronu (`04 28`).** Ayrıntı: `donanim-dogrulama.md`, "Saat senkronu".
    - **Protokol:** `kb-protocol` saat paketini kodluyor (S2'nin yakalamasıyla birebir test edildi). Sürücü `04 18 → 04 28 → veri → 04 02` akışını uyguluyor.
    - **Doğrulama farkı:** Bu akışta `04 02` bayt toplamını 0 bildiriyor; teslimat veri paketinin birebir yankısıyla doğrulanıyor.
    - **Uygulama:**
      - Klavye USB'den bağlanınca 2 sn bekleyip saati ayarlıyor; günde en fazla bir kez, çünkü her saat paketi ekranı saat sayfasına geçiriyor. Başarısız olursa 5 sn arayla 3 kez deniyor.
      - Ayar ve son eşitleme zamanı `clock.json`'da (`settings.rs`, kalibrasyonla ortak sürümlü dosya yardımcıları).
      - Genel Bakış'ta "Klavye saati" kartı var. Yerel saat `chrono` ile alınıyor (zaten bağımlılık ağacındaydı).
    - **CLI:** `kb clock [--at "YYYY-MM-DD HH:MM:SS"]`.
18. **Efekt yönleri ölçüldü.** Kaynaklar çelişkiliydi; göz testi yerine `kb direction-probe` ile ölçüldü. Ayrıntı: `donanim-dogrulama.md`, "Efekt yönleri".
    - Beş yönlü efektin (10, 11, 12, 16, 18) her birinde yalnızca iki yön var; kod 2 ve 3, 1 ve 0'ı tekrarlıyor. Mod 10'un "4 yön"ü bu firmware'de yok.
    - Katalog her efektin hareketini taşıyor (`Motion`: dikey, yatay, dönme, yol, çapraz). Arayüz yön düğmelerini buna göre adlandırıyor; eski kayıtlardaki 2/3 değerleri doğru düğmede görünüyor.
    - Ölçüm aracı: F5 karelerinden desen kaymasını hesaplayan `kb-cli/src/motion.rs` (eksen izdüşümü ve iki boyutlu eşleştirme, birim testli).
    - Kullanıcı yön düğmelerini uygulamada seçip klavyede denedi: doğru çalışıyor.
    - **PR'lar:** [#6](https://github.com/cebersaban44-rgb/klavye-studio/pull/6) madde 15, [#7](https://github.com/cebersaban44-rgb/klavye-studio/pull/7) madde 16, [#8](https://github.com/cebersaban44-rgb/klavye-studio/pull/8) madde 17, [#9](https://github.com/cebersaban44-rgb/klavye-studio/pull/9) bağlantı modları listesi, [#10](https://github.com/cebersaban44-rgb/klavye-studio/pull/10) madde 18.
19. **Tuş atama** (`04 11` temel, `04 27` Fn). Ayrıntı: `donanim-dogrulama.md`, "Tuş atama".
    - **Protokol** (`kb-protocol/src/sonix/keymap.rs`): `KeyAction` (varsayılan, devre dışı, tuş + değiştiriciler, medya, fare, tekerlek), 576 baytlık katman tablosu, HID ve medya kataloğu. Fn (`0xAF`) hedef olarak da kabul edilmez. Geçerli her girişin `guard`'dan geçtiği testle kanıtlı.
    - **Model:** her tuşun varsayılan HID kodu (`hid`) model dosyasında. `kb-core/src/keymap.rs` katmanı modele göre denetler: Fn tuşu kilitli, modelde olmayan tuş reddedilir, temel katmanda "tuş kendini gönderir" varsayılana çevrilir.
    - **Sürücü:** `04 18` → `04 11/27` (9) → 9 veri → `04 02` (sağlama toplamı) → `04 F0` (yankısı okunuyor, resmi sürücü gibi).
    - **Uygulamanın kaydı:** Klavye atamayı geri okuyamıyor. Cihaz aktörü yalnızca klavyenin onayladığı yazımdan sonra `keymap.json`'u (model başına, katman başına, ham 4 baytlık girişler ve son yazım zamanı) günceller. Uygulama açılınca bu kayıt yüklenir; kaydedilmemiş taslak `ks.keymap.v1`'de durur ve kapatma uyarısına katılır.
    - **Arayüz:** Tuş Atama sayfası: katman sekmeleri, klavyede seçim (değişen tuş yeni işlevini ve altında asıl etiketini gösterir, kaydedilmemişler noktalı), işlev seçici (Tuşlar + "Birlikte basılı" değiştiriciler, Medya, Fare, Özel), kaydet paneli ve okunamama/Fn katmanı notları, tuş testi kutusu.
    - **CLI:** `kb keymap --layer base|fn KEY=ACTION … --yes`, `kb keymap-targets`.
    - **Doğrulama:** CLI ile iki katman yazılıp kullanıcı bastı; varsayılana dönüş doğrulandı. Sonra uygulamanın kendi Kaydet düğmesiyle yazılıp sayfadaki tuş testi kutusunda doğrulandı ve uygulamadan fabrika ayarına döndürüldü. Klavye şu an fabrika atamasında.
    - **PR'lar:** [#12](https://github.com/cebersaban44-rgb/klavye-studio/pull/12) madde 19, [#13](https://github.com/cebersaban44-rgb/klavye-studio/pull/13) "tuşla uygulama başlatma" fikri (P2).
20. **Makrolar: temel** (`04 19` → `04 15`). Ayrıntı: `donanim-dogrulama.md`, "Makrolar".
    - **Protokol** (`kb-protocol/src/sonix/macros.rs`): makro görüntüsü (100 yuvalık dizin, kayıtlar, sayfa sayısı ve `AA 55`), bellek sınırı (`(adım + yuva) × 8 ≤ 0xA8C`), ABD düzeniyle metin → adım, geri okuma (test için). Geçerli her görüntünün `guard`'dan geçtiği testle kanıtlı.
    - **Yuvalar sabit:** Boş yuva numarasını korur (`FF FF FF FF`). Bir makro silinince sonrakiler kaymaz, tuş atamalarını yeniden yazmak gerekmez.
    - **Tuşa bağlama:** `KeyAction::Macro { slot, play }` = `06 SLOT MODE COUNT`; bir kez, N kez, tekrar basana kadar, basılı tutulduğu sürece. Satıcının 4. modu ("click to play") anlamı belirsiz olduğu için sunulmuyor.
    - **Sürücü:** `04 19` → `04 15` (sayfa) → 30 ms → sayfalar → 30 ms → `04 02`; `04 18` ve `04 F0` yok. `04 02` yankısındaki toplam görüntünün toplamıyla tutuyor.
    - **CLI:** `kb macros SPEC… --yes` (`text:`, `combo:`, `steps:`; `-` boş yuva), `kb keymap KEY=macro:N[:times:K|:toggle|:hold]`.
    - **Doğrulama:** Kullanıcı Num / (bir kez "ok"), Num * ("ok" üç kez) ve Num − (sağ tık, 200 ms, x) tuşlarına bastı; olaylar ve beklemeler beklendiği gibi. Sonra makrolar silindi ve atama fabrika ayarına döndü.
    - **PR:** [#14](https://github.com/cebersaban44-rgb/klavye-studio/pull/14).
21. **Makrolar sayfası.** Ayrıntı: `donanim-dogrulama.md`, "Makrolar sayfası, uçtan uca".
    - **Uygulamanın kaydı:** `macros.json` (model başına yuvalar; ad ve adımlar, son yazım zamanı). Klavye makroları ve adlarını geri okuyamıyor; cihaz aktörü kaydı yalnızca klavyenin onayladığı yazımdan sonra günceller. Taslak `ks.macros.v1`'de, kapatma uyarısına katılıyor.
    - **Sayfa:**
      - Makro listesi (M1…M100, ad, adım, süre, kaydedilmedi noktası) ve bellek göstergesi (`(adım + yuva) × 8` / 2700).
      - Düzenleyici: ad; "Kayda başla" ile tuşlar `e.code` (konum) üzerinden HID koduna çevrilerek eklenir. Gerçek ya da sabit bekleme seçilebilir.
      - Kayıt sırasında tekrarlar yok sayılır; kayıt durunca basılı kalan tuşlar bırakılır.
      - Fare tıklaması ekleme, adım başına bekleme, taşıma ve silme; "bırakılmayan tuş" uyarısı.
      - Silme: yuva numarası korunur. Bağlı tuşlar listelenir ve bağlantıları da kaldırılır.
    - **Tuş Atama:** "Makro" kategorisi; oynatma biçimi (bir kez, belirli sayıda, tekrar basana kadar, basılı tuttukça). Tuş üstünde `M1`, panelde makronun adı görünür.
    - **Klavye düzenine göre etiketler** (`lib/layout.ts`): WebView2'nin Keyboard Map API'siyle her tuşun Windows'ta yazdığı karakter gösteriliyor (Türkçe Q'da `;` konumu **Ş**, `[` **Ğ**). Makro adımlarında, Tuş Atama seçicisinde ve atanmış tuş adlarında kullanılıyor. Pencere odak alınca düzen yeniden okunuyor.
    - **Doğrulama:** Kayıt, bağlama ve silme CDP ile denendi. Ardından uygulamadan makro ve atama kaydedildi; kullanıcı Num /'a bastı ve tuş testi kutusunda "Shift+H, Ş, I, sol tık" dizisi tekrar tekrar geldi. Sonra makro uygulamadan silindi ve ikisi de kaydedildi; klavye ve kayıtlar fabrika durumunda.
    - **PR:** [#15](https://github.com/cebersaban44-rgb/klavye-studio/pull/15).
22. **Ekran: derin araştırma ve güvenli yükleme çekirdeği.** Ayrıntı: `research/ekran-derin-arastirma.md`, `donanim-dogrulama.md` "Ekran yüklemesi".
    - **Araştırma** ([#17](https://github.com/cebersaban44-rgb/klavye-studio/pull/17)): resim ve GIF evet; canlı video yok, kısa video ≤140 karelik animasyona çevrilir; tek animasyon yuvası; stok firmware'de flash'sız yol yok; kullanıcının "kendi firmware katmanımız" fikri araştırma hattı.
    - **Fabrika animasyonu yedeği:** Resmi sürücü paketinden çıkarıldı (hiçbir şey çalıştırılmadan); kullanıcı klavyedekiyle aynı olduğunu doğruladı.
    - **Protokol** (`kb-protocol/src/sonix/screen.rs`):
      - `Frame` (240×135 RGB565) ve `ScreenAnimation`. Klavyeye giden tek tür bu: 1–140 kare, gecikme 1–254 (`FF` yasak).
      - Blob ve sayfalar, sayfa tavanı (2215), test deseni.
      - `guard_screen_page`; `04 38`, `1F`, `50`, `51` engelli (testle).
    - **Aktarım:** FF68 kanalında GET_FEATURE reddediliyor (kardeş kartta firmware'i kilitliyor).
    - **Sürücü** (`SonixFf13::upload_screen`):
      - `04 18` → `04 72 01 … PAGES` → her sayfa WriteFile ile ve onayı okunarak → `04 02`. `04 F0` yok.
      - Sayfalar göndermeden önce denetleniyor. Bir sayfa başarısız olursa oturum `04 02` ile kapatılıyor.
    - **İçe aktarma** (`kb-core::screen_import`): GIF, boyut ve bellek sınırlarıyla çözülüyor, kareler birleştiriliyor (disposal). Şimdilik yalnızca 240×135; ölçekleme Ekran Stüdyosu'nda.
    - **CLI:** `kb screen-test [--preview PNG] --yes`, `kb screen-gif DOSYA [--delay-ms N] [--preview PNG] --yes`.
    - **Doğrulama:**
      - E1: test deseni doğru yön ve renkte, ayrı GIF sayfasında; Fn+Home ile saat sayfasına geçiliyor.
      - E4: fabrika animasyonu 308 s'de geri yüklendi; görüntü ve hız aynı. Yükleme sırasında ekranda LOADING % görünüyor.
    - **PR:** [#18](https://github.com/cebersaban44-rgb/klavye-studio/pull/18).
23. **Ekran Stüdyosu sayfası.** Ayrıntı: `donanim-dogrulama.md`, "Ekran Stüdyosu, uçtan uca".
    - **Kullanıcının ilkesi uygulandı:** Dosya olduğu gibi gönderilmez. WebView2'nin kendi çözücüleriyle açılır, 240×135 RGB565'e çevrilir, gönderilecek baytlardan önizlenir, onayla yüklenir. Rust tarafı (`screen_add`) veriyi yeniden doğrular.
    - **Akış:** Dosya → Düzenle → Önizle → Yükle; her adımda Vazgeç. Bitince alan başlangıca döner ve ne yapıldığını söyler.
      - Düzenle: sürükleyerek kaydırma, tekerlekle yakınlaştırma; Doldur / Sığdır / Uzat; video ve animasyonda başlangıç, bitiş ve kare hızı; Bayer yumuşak geçiş. Renkler panelin gösterebildiği renkler.
    - **Kütüphane** (`<config>/screen/`):
      - Hazırlanan her animasyon doğrulanmış blob olarak saklanır; içerik özeti (FNV-1a) aynı içeriği tekilleştirir.
      - Fabrika yedeği kendiliğinden eklenir ve silinemez.
      - Yükleme, yeniden adlandırma ve iki adımlı silme var. "klavyede" etiketi, GIF sayfasında ne olduğunu gösterir.
      - Bugünkü ve toplam yükleme sayısı tutulur.
    - **Yükleme:** Cihaz aktörü üzerinden; `screen-upload-progress` olaylarıyla ilerleme gösterilir. Başka sayfalarda yüzen bir gösterge çıkar. Yükleme sürerken kapatmaya çalışınca uyarı verilir.
    - **Klavye görünümü:** Saatli ana sayfanın canlı kopyası ile GIF sayfasının önizlemesi yan yana; Fn+Home açıklaması.
    - **RGB565 dönüşümü:** Rust ve arayüz aynı kuralı kullanıyor: en yakın seviye (kırpma değil).
    - **Doğrulama:** CDP ile fabrika GIF'i, dikey fotoğraf ve WebM video içe aktarıldı. Kullanıcı kendi resim ve GIF'ini yükledi; E2 geçti; yükleme sırasında yazma sürüyor.
    - **Henüz yok:** slayt, canlı kartlar. Yazı kartları madde 24'te.
    - **PR:** [#19](https://github.com/cebersaban44-rgb/klavye-studio/pull/19).
24. **Ekran Stüdyosu Faz B1: kaynak seçici ve yazı kartları.**
    - **Akış:** Adım 1 artık kaynak seçiyor: **Dosyadan** (Faz A akışı) ya da **Yazı kartı**. Adımlar: Kaynak → Düzenle → Önizle → Yükle.
      - Önizleme, onay, ilerleme ve "bitti" durumu bütün kaynaklarda ortak (`pages/screen/common.tsx`).
      - "Düzenlemeye dön" ayarları korur. Son yazı kartı oturum boyunca hatırlanır.
    - **Şablonlar:** ad kartı, söz, kayan yazı, dağ teması (çerçevedeki dağlara uygun çizgi), rozet (emoji ya da 1–2 harf).
      - Kullanıcı yazmaya başlayana kadar şablon değişince örnek yazı da değişir; yazdıktan sonra kendi yazısı korunur.
    - **Ayarlar:**
      - başlık ve alt yazı;
      - yazı tipi: Bricolage, Instrument, Mono;
      - büyük harf (uygulamanın diline göre; Türkçede "i" → "İ");
      - boyut (sığmazsa küçültülür ve bunu gösterir);
      - hizalama (yalnız işe yaradığı yerde);
      - altı renk teması ("Gece" klavyenin kendi ana sayfasının renkleri) ve elle renk seçimi;
      - düz ya da degrade arka plan, degradenin yönü;
      - kayan yazıda hız ve kare hızı.
    - **Çizim** (`lib/screenText.ts`):
      - Arka plan ve yazı iki ayrı katmanda çizilir. Yalnız arka plan pikselleri yumuşatılır (Bayer), yazı keskin kalır (`toRgb565` artık piksel maskesi alıyor).
      - Fontlar çizimden önce yüklenir (`document.fonts.load`, Türkçe harfler dahil).
      - Rozet işareti çizilen piksellerine göre ortalanır; renkli emoji fontlarının ölçüleri güvenilir değil.
      - Kayan yazı tam piksel adımlarla kayar. Döngü 140 kareye sığmazsa hız yükseltilir ve kullanıcıya söylenir.
      - Kayan yazıda varsayılan kare hızı 10: fabrika animasyonunun kanıtlanmış hızı. Daha yükseği E3 ölçülene kadar seçmeli.
    - **Uyarılar:** yazı ile arka plan arasında kontrast düşükse; 135 kareden uzunsa.
    - **Doğrulama:** Tarayıcıda sahte arka uçla (`?mock=usb`, 1320×860) bütün yollar denendi:
      - beş şablon;
      - Türkçe harfler ve büyük harf;
      - yükleme → "bitti" bandı;
      - yüklemeden kaydet;
      - bozuk dosya hatası;
      - Vazgeç;
      - düzenlemeye dön.
      - Klavyeye gerçek yükleme henüz yapılmadı; kullanıcının onayını bekliyor.
    - **PR:** [#21](https://github.com/cebersaban44-rgb/klavye-studio/pull/21).
25. **Ekran Stüdyosu Faz B2: slayt.**
    - **Kaynak:** Adım 1'de üçüncü kutu, **Slayt**. Birkaç görsel tek döngüde gösterilir.
    - **Slaytlar** (en çok 24):
      - Dosyadan eklenebilir: çoklu seçim ya da önizlemenin üstüne bırakma. Animasyon ya da videodan en dolu an alınır: 8 an örneklenir, en parlak olan seçilir.
      - Kütüphaneden eklenebilir: öğenin temsilci karesi alınır.
      - Her slaytın süresi 1–20 sn. Dosya slaytları Doldur ya da Sığdır olabilir.
      - Sıralama yukarı/aşağı yapılır, slayt çıkarılabilir. Degrade yumuşatma açılıp kapatılabilir.
    - **Geçiş:**
      - Kesme;
      - Yumuşak: 3 karışım karesi, her biri 100 ms. Son slayttan ilkine de geçiş var, döngü dikişsiz.
    - **Kare bütçesi:**
      - Panel bir kareyi en çok 508 ms gösterir. Bu yüzden D saniyelik slayt ⌈D / 0,508⌉ kez tekrarlanan aynı karedir.
      - Düzenleyicide "Kare bütçesi" çubuğu var.
      - 140 kare aşılırsa Önizle kapanır ve nedeni yazılır.
      - Doğrulandı: `assemble` ile `budget` her durumda aynı kare sayısını ve döngü süresini veriyor; en uzun kare 500 ms.
    - **Ad:** Varsayılan "Slayt · N görsel"; kullanıcı değiştirmedikçe slayt sayısını izler.
    - **Ayrıca:**
      - "Yumuşak geçiş" anahtarının adı "Degrade yumuşatma" oldu. Slayttaki "Yumuşak" geçişle karışıyordu; zaten yaptığı iş bantlanmayı azaltmak.
      - Küçük resim ve en parlak kare yardımcıları ortak modüllere taşındı.
    - **Doğrulama:** Tarayıcıda sahte arka uçla (1320×860) denendi:
      - dosya ekleme (çoklu ve iki seferde);
      - kütüphaneden ekleme;
      - yukarı taşıma, çıkarma;
      - 20 sn'lik süreler ve bütçe aşımı;
      - önizleme → yükleme → "bitti" bandı;
      - Vazgeç.
      - Geliştirme sırasında bir hata bulunup düzeltildi: HMR sonrası slayt anahtarları çakışıp yanlış küçük resim gösteriyordu. Anahtarlar artık düzenleyici başına sayaçla üretiliyor.
      - Klavyeye gerçek yükleme henüz yapılmadı.
    - **PR:** [#22](https://github.com/cebersaban44-rgb/klavye-studio/pull/22).
26. **Ekran Stüdyosu Faz B3: ışıkları ekrana uydur.**
    - **Nereden:** "Bitti" bandındaki "Işıkları uydur" düğmesi ve kütüphanedeki her öğenin palet simgesi (fabrika animasyonu dahil).
    - **Palet** (`lib/screenPalette.ts`):
      - En çok 6 kareden örnek alınır, median cut ile 10 kutuya bölünür.
      - Kutular "pay × (0,25 + doygunluk) × (0,35 + parlaklık)" ile sıralanır; LED'de aynı görünecek renkler elenir, en çok 5 renk kalır.
      - Siyaha yakın pikseller (LED'de "sönük") palete alınmaz.
      - LED'in parlaklığı klavyenin kendi ayarı olduğu için her renk tam değere çıkarılır. Pencere iki hâli yan yana gösterir: görseldeki renk ve tuştaki hâli.
    - **Üç tasarım:**
      - **Degrade:** palet, renklerin görseldeki yatay sırasıyla soldan sağa.
      - **Tek renk:** en baskın renk bütün tuşlarda.
      - **Görsel:** her tuş, görselde kapladığı alanın ortalamasını alır. Bütün tuşlara tek kazanç uygulanır; siyah yerler sönük kalır.
    - **Yazma yok:**
      - "Işıklar'da aç" tasarımı Işıklar sayfasında "Tuş başına renk" sekmesinde kaydedilmemiş taslak olarak açar (`useLighting` `setPerKey` + `setTab`).
      - Canlı önizleme tuşlarda RAM'den görünür. Flash'a yazım yalnızca kullanıcı orada "Klavyeye kaydet"e basınca olur.
      - Kaydedilmemiş değişiklik uyarısı her zamanki gibi çalışır.
      - Vazgeç ve Esc hiçbir şeyi değiştirmez.
    - **Doğrulama:** Tarayıcıda sahte arka uçla denendi:
      - "Gün batımı" dağ kartından 5 renklik palet;
      - üç seçeneğin önizlemesi;
      - Işıklar'da açınca 97 tuşluk taslak ve canlı önizleme göstergesi;
      - kütüphaneden açma, Vazgeç, Esc.
    - **Gerçek uygulamada (CDP):**
      - Bir yazı kartı ve iki görsellik bir slayt "Yüklemeden kaydet" ile kütüphaneye kaydedildi; Rust tarafı (`screen_add`) ikisini de kabul etti. Klavyeye yazılmadı; deneme öğeleri sonra silindi.
      - Bulunan kusur düzeltildi: slaytın paletine geçiş karelerinin karışım renkleri giriyordu (kırmızı + mavi → pembe, mor). Palet artık uzun duran karelerden çıkarılıyor (≥300 ms; daha kısa kare varsa).
    - **Klavyede (kullanıcı, 2026-11-10):** Işıklar yandı ama renkler görsele benzemedi; "cat" doğruydu, mor "Günaydın" kartı (Neon) değildi.
      - **Teşhis:**
        - Klavye canlı kareleri kabul ediyor; kayıtlı ışık "Kapalı" iken de (`kb live-probe`, F5 ile doğrulandı).
        - Uygulama günlüğündeki birkaç `04 20` reddi, bilinen ara sıra görülen `FF`.
        - Asıl sorun paletteydi:
          - beyaz yazı palete giriyordu, degradenin ortası beyaz yanıyordu;
          - renkler görseldeki yatay yerlerine göre sıralanıyordu, ortalı yazı kartında bu sıra rastgele;
          - "Görsel" en parlak yere (beyaz yazıya) göre ölçeklendiği için koyu mor alanlar sönük kalıyordu.
      - **Düzeltme:**
        - Görselde gerçek renk varsa (doygunluk ≥ 0,2, pay ≥ %1) beyaz, gri ve siyaha yakın kutular palete alınmıyor.
        - Sıralama puanı doygunluğu daha çok tartıyor.
        - Degrade, renkler görselde birbirinden ayrık durmuyorsa (yatay yayılım < 0,25) tona göre diziliyor; en geniş ton boşluğundan başlanıyor.
        - "Görsel" seçeneği parlaklığı %90'lık dilime göre, karekök eğrisiyle kaldırıyor.
      - **Sonuç:** "Günaydın" paleti mor #791BFF, pembe #FF3FD7, açık pembe #FA8CFF. Kullanıcı klavyede degradeyi gözle "karta benziyor" diye doğruladı. Testten sonra kullanıcının ışık tasarımı (kayıtlı 16 tuş) geri yüklendi.
27. **Ekran Faz C, ilk adım: canlı mod, yükleme bütçesi, "Çalan şarkı" kartı.**
    - **Bütçe** (`app/src-tauri/src/live.rs`):
      - Token bucket: Tasarruf 8/gün (art arda en çok 3), Dengeli 24/gün (6, varsayılan), Yoğun 48/gün (8). Saatlik dolum = günlük / 24.
      - Elle yapılan yüklemeler de bütçeden düşer ama hiç engellenmez. Borç bir kova ile sınırlı.
      - Flash'a hiç dokunmayan başarısız deneme bütçeyi geri verir.
      - Ayarlar ve bütçe `library.json` içinde `live` alanında tutulur. Alanın eksik olması sorun değil; sürüm 1 kaldı.
    - **Canlı yükleme işçisi:**
      - Yalnız en yeni bekleyen kart tutulur.
      - GIF sayfasında zaten aynı içerik varsa (içerik özeti) yüklenmez.
      - Bütçe, kablo ya da başarısız deneme yüzünden gidemeyen kart bekler: bütçe dolunca ya da 10 sn sonra yeniden denenir.
      - Kart yine Rust'ta doğrulanır (`live_submit`). Son kart `screen/live.bin`'e yazılır; GIF sayfası önizlemesi onu gösterir (`shown = "live"`).
    - **"Çalan şarkı" kaynağı** (`now_playing.rs`):
      - Windows'un medya oturumları (GSMTC) saniyede bir yoklanır, yalnız canlı mod ve kaynak açıkken.
      - Çalan oturum, duraklatılmış olana tercih edilir.
      - Şarkı 12 sn kesintisiz çalınca kapağıyla birlikte duyurulur (`now-playing` olayı); hızlı atlamalar yüklenmez.
      - Bilinen uygulamalar adıyla gösterilir (Spotify, Chrome, Edge, Medya Oynatıcı…).
    - **Kart** (`lib/liveCards.ts`):
      - Solda kapak, sağda "ÇALIYOR", başlık (iki satıra sığacak kadar büyük), sanatçı ve uygulama.
      - Arka plan kapağın renk tonundan koyu bir degrade. Arka plan yumuşatılır, yazı keskin kalır.
      - Saat ya da ilerleme gibi bayatlayan bilgi yok.
    - **Arayüz:** Ekran sayfasında "Canlı ekran" bölümü: canlı mod anahtarı, günlük bütçe ve kalan yükleme, kaynak anahtarı ve o an çalan, durum (yükleniyor / sırada ve neden / ekranda).
    - **Doğrulama:**
      - Bütçe birim testleri (dolum, art arda sınır, elle yükleme borcu, iade, plan değişimi, eski kayıt).
      - Tarayıcıda sahte arka uçla örnek şarkı: kart çizildi, bütçeden düştü, GIF sayfası önizlemesi kartı gösterdi.
      - Bu PC'de GSMTC okundu (salt okuma testi, `now_playing_probe`): Chrome'da duraklatılmış bir YouTube videosu, başlık, kanal ve 24 KB kapak; duraklatılmış olduğu için "çalıyor" sayılmadı.
      - **Klavyede (kullanıcı, 2026-11-10):** Chrome'da çalan bir video kartı "çok güzel" geldi. Günlükte 12 sn beklemeden sonra kart 55 ms'de çizildi, yükleme 2,8 sn sürdü (16 sayfa).
    - **Kullanıcının geri bildirimi ve düzeltmeleri (aynı gün):**
      - **"Canlı mod kapanınca eski görsel geri gelmeli":**
        - İlk canlı kart, GIF sayfasında önceden ne olduğunu hatırlıyor (`live.before`; bilinmiyorsa fabrika animasyonu).
        - Canlı mod kapatılırken ekranda canlı kart varsa bir pencere soruyor: "Önceki görseli geri yükle" (birincil) ya da "Kartı bırak". Görselin adı, kare sayısı ve yükleme süresi gösteriliyor.
        - Geri yükleme kullanıcının isteği sayılır: bütçeden düşer ama bütçeyi beklemez. Kablo yoksa bekler ve sonra yapılır.
        - Canlı mod zaten kapalıyken ekranda kart kaldıysa Canlı ekran bölümünde aynı seçenek bir satır olarak duruyor.
        - Kullanıcının mevcut durumu: ilk kart eski kodla yüklendiği için önceki görsel kaydedilmemişti. `library.json`'a elle `before = "Slayt · 3 görsel"` yazıldı (yedek: `library.json.bak-before-restore-fix`).
      - **"12 sn geç değil mi?":** Haklı. Bekleme 4 sn'ye indi. Kapak geç gelirse en çok 3 yoklama (3 sn) daha beklenir. Şarkının başlangıcından ekranda görünmesine kadar artık yaklaşık 7 sn (4 sn bekleme + 3 sn yükleme).
      - **Kullanıcı ikisini de klavyede denedi, "başarılı":**
        - Kart 4 sn beklemeyle geldi (yükleme 2,7 sn).
        - Canlı mod kapanınca "Önceki görseli geri yükle" seçildi; kullanıcının elle yüklediği "cat" 37 sn'de geri geldi.
    - **Arada bulunan hatalar:**
      - CI'da rustfmt hatası. Ders §6: push etmeden önce CI adımlarını yerelde çalıştır.
      - Uygulama dal değişimi sırasında açılınca pencere boş kaldı. Ders §6.
    - **PR:** [#25](https://github.com/cebersaban44-rgb/klavye-studio/pull/25) (canlı mod, bütçe, çalan şarkı), [#26](https://github.com/cebersaban44-rgb/klavye-studio/pull/26) (kapatınca geri yükleme, 4 sn).
28. **Ekran Faz C, ikinci adım: Odak zamanlayıcısı (Pomodoro tekniği).**
    - **Ad:** Arayüzde "Odak zamanlayıcısı" (İngilizcede "Focus timer"). Kullanıcı "Pomodoro"nun Türkçe olmadığını fark etti: kelime İtalyanca ("domates"), TDK sözlüğünde yok ve tekniğin adı olarak Francesco Cirillo'nun tescilli markası. Türkçe sayfada yalnız Türkçe kelime kullanılır (ders §6, 41). Koddaki adlar (`pomodoro.rs`, `source = "pomodoro"`) iç ad olarak kaldı.
    - **Zamanlayıcı arka uçta** (`app/src-tauri/src/pomodoro.rs`):
      - Neden: pencere küçülünce WebView2 sayfanın zamanlayıcılarını kısıyor (5 dk sonra dakikada bir). Süre Rust'ta tutulursa faz değişimi ve ışıklar şaşmaz.
      - Fazlar: Odak → Kısa mola → … → son odaktan sonra Uzun mola → seri biter. Varsayılan 25 / 5 / 15 dk, 4 tur. Ayarlar `pomodoro.json`'da (sürüm 1).
      - Başlat, Duraklat / Sürdür, Atla (fazı bitir, sonrakine geç), Bitir.
      - Süre değişikliği süren faza da hemen uygulanır.
      - Bilgisayar uzun süre uyursa yalnızca bir faz ilerler; yeni faz uyanınca başlar.
      - Atlanan odak tur sayılır ama özetteki odak süresine yalnız geçen süre girer.
    - **Tuş ışıkları** (flash'sız, `04 20`):
      - Cihaz aktörüne ikinci bir canlı katman eklendi (`SetAmbient`). Işıklar sayfasının önizlemesi (`SetLive`) her zaman üstte; kapanınca Pomodoro katmanı geri gelir.
      - F1–F12 fazın rengiyle dolar: odak turuncu, kısa mola yeşil, uzun mola mavi. Dolmamış kısım soluk (%8); dolan tuş 32 adımda parlar, böylece kare yalnız görünen bir şey değişince gönderilir.
      - Esc fazın rengini gösterir. Rakam tuşları 1–N biten turları sayar.
      - Faz başında bütün klavye yeni fazın rengiyle iki kez yanıp söner (1 sn). Duraklatınca çubuk yavaşça yanıp söner. Seri bitince iki yeşil yanıp sönme, sonra katman durur ve klavye kendi efektine döner.
      - F sırası olmayan modellerde çubuk rakam sırasına geçer (model dosyasındaki tuş kimliklerinden).
    - **Ekran kartı** (canlı modun kaynağı, `source = "pomodoro"`):
      - Faz başında, yanıp sönme bittikten sonra arayüzden kart istenir (`pomodoro-card` olayı); yükleme ışık akışını kesmesin diye beklenir. Kart canlı modun kurallarıyla gider: bütçe, en yeni kazanır.
      - Kart: faz adı, süre ("Odak 25:00"), sağda tur halkası ("2/4", biten turlar dolu), altta sıradaki faz ("Sonra: 5 dk mola"). Geri sayım yok: kart faz boyunca doğru kalır, ilerleme tuşlarda.
      - Pomodoro çalışırken ekranı tutar: "Çalan şarkı" duyurmaz. Pomodoro bitince çalan şarkı varsa kartı kendiliğinden geri gelir; yoksa bir özet kartı yüklenir ("Seri tamam · 100 dk"; erken bitince "Pomodoro bitti", halkada biten turlar ve bitiş saati). Böylece ekranda bayat bir "Odak" kartı kalmaz (kullanıcının madde 27'deki "kapanınca eskisi gelsin" ilkesi).
      - Canlı mod kapalıysa kart gitmez; ilerleme yalnız tuşlarda.
      - Maliyet: her faz bir yükleme, 4 turluk seri 8 yükleme. Panel bunu bütçeyle karşılaştırır, yetmezse uyarır.
    - **Arayüz:**
      - Ekran sayfasında "Pomodoro" bölümü: kalan süre halkası, tuşların küçük kopyası (Esc, F1–F12, tur tuşları; klavyedekiyle aynı kural), düğmeler, süreler (− / + ve yazarak), iki anahtar ("Tuş ışıklarında ilerleme", "Faz başında ekran kartı").
      - Canlı ekran bölümünün kaynaklarında "Pomodoro" satırı (kart anahtarı ve durum). Pomodoro sürerken "Çalan şarkı" satırı "bekliyor" der.
      - Pomodoro çalışırken kenar çubuğunda faz, tur ve kalan süre görünür; tıklayınca bölüme gider.
    - **Doğrulama:**
      - 11 birim testi: seri akışı, duraklatma, erken bitirme, atlama, uyku, süre değişikliği, ayar sınırları, AK980'de F1–F12 = LED 2–13, çubuğun dolması, duraklama yanıp sönmesi, bitiş yanıp sönmesi.
      - Tarayıcıda sahte arka uçla (`?mock=usb&pomofast`, bir dakika 2 sn): bütün seri; duraklat / sürdür / atla / bitir; kartlar ve özet kartı; açık ve koyu tema; 1024 px genişlik.
      - **Gerçek uygulamada, klavyede** (CDP, canlı mod kapalı, yani yükleme yok):
        - 1 dakikalık odak; duraklatınca süre durdu; süre bitince kısa molaya geçti; Bitir'le katman durdu.
        - Işıklar sayfası önizlemesi Pomodoro'nun üstüne geldi, kapanınca çubuk geri geldi.
        - Ayrıntı: `donanim-dogrulama.md`, "Pomodoro ışık katmanı".
      - Tek tek reddedilen `04 20` kareleri (yaklaşık %1) artık `debug` düzeyinde. Yalnız art arda 20 kare (1 sn) giderse uyarı yazılır. Yoksa 25 dakikalık bir odakta yüzlerce uyarı satırı birikirdi.
    - **Bekleyen:** Kullanıcının ışıkları gözle görmesi ve canlı mod açıkken faz kartlarının klavyeye yüklenmesi (flash yazar; kullanıcının kendi denemesiyle). Pencere küçükken kart çiziminin gecikip gecikmediği de o denemede görülecek.
    - **Kullanıcının ışık tasarımı (aynı gün, ilk denemeden sonra):**
      - Geri bildirim: "Yalnız en üst sıra yanınca odaklanma hissi vermiyor." Bütün klavye fazın rengiyle dolu başlamalı ve süre azaldıkça bir yöne doğru boşalmalı. Kalan ışık kalan zamandır; dolu klavye "çalış", azalan ışık "molaya az kaldı" der. Yön ve renkler seçilebilmeli: kırmızı uzun sürede rahatsız edebilir.
      - Önce sohbette etkileşimli bir önizleme gösterildi (gerçek tuş dizilimiyle). Kullanıcı "O zaman uygula" dedi.
      - **Yapılan** (F1–F12 çubuğu, Esc ve tur tuşları kalktı):
        - Bütün 97 tuş. Işığın bittiği kenar yumuşak (klavye genişliğinin %4'ü), yani hareket akıyor.
        - Boşalan kısım soluk (%8).
        - Hareket: sağdan sola (varsayılan; ışık solda kalır), soldan sağa, yukarıdan aşağı (üst sıralar önce söner), ortaya doğru (iki yan önce söner), yavaşça söner (yön yok).
        - Odak, kısa mola ve uzun mola renkleri seçilir (paletten ya da serbest renk). Parlaklık klavyenin kendi basamaklarıyla 1–5 (varsayılan 4).
        - Son dakikada nabız (açılıp kapatılır): bütün klavye birlikte nefes alır. Kalan ışık biraz söner, sönük kısım biraz parlar; son dakikada yalnızca birkaç tuş yanık kaldığı için ilk denemede yalnız yanık tuşların nefes alması fark edilmiyordu (testte yakalandı).
        - Duraklatınca bütün klavye kısılır ve yavaş nefes alır.
        - Faz başında: iki kez yanıp sönme ya da dalga (yeni renk, en uzun yanık kalan taraftan akar).
        - Faz bitince ses (varsayılan kapalı): Windows'un bildirim sesi (`%SystemRoot%\Media\Windows Notify System Generic.wav`). Çalan, WinRT `MediaPlayer`; kodda `unsafe` yasak olduğu için Win32 ses çağrısı kullanılmadı. Yalnızca süre kendiliğinden bitince çalar, düğmelerde çalmaz.
        - **"Klavyede dene":** seçilenleri klavyede 12 sn'lik bir turla gösterir (8 sn odak, 3 sn mola, bitiş). Yalnızca zamanlayıcı dururken.
        - Uygulamadaki önizleme klavyenin tuş dizilimiyle, tuşlar düz renkle çizilir. Arka uçla aynı kurallar `lib/pomodoroLights.ts`'te; ikisi birlikte değiştirilmeli. Gerçekçi "alttan parlama" çizimi bu boyutta etkiyi göstermedi (ders §6, 42).
        - Halka da artık kalan süreyi gösteriyor (dolu başlar, azalır). Renkler kenar çubuğuna, karta ve halkaya da uygulanıyor.
        - Eski `pomodoro.json` dosyaları yeni alanları varsayılanla alıyor (testli).
      - **Doğrulama:** 16 birim testi (yönler, renkler, nabız, duraklama, geçişler, deneme turu, eski ayar dosyası). Tarayıcıda sahte arka uçla bütün seçenekler. Gerçek uygulamada iki "Klavyede dene" turu klavyede çalıştı: önce varsayılanlarla ve ses açık, sonra soldan sağa, dalga ve mor odakla. Ayrıntı: `donanim-dogrulama.md`.
      - **Bekleyen:** Kullanıcının gözle denemesi ve sesi duyması.

29. **Ekran sayfasının düzeni: sekmeler, birleşik odak zamanlayıcısı, büyüyen kütüphane.**
    - **Kullanıcının geri bildirimi (2026-11-10):**
      - Odak zamanlayıcısı ile tuş ışıkları iki ayrı parça gibi duruyordu; ışıklar zamanlayıcının ayarları gibi görünmeli.
      - Kütüphane en alta kaymıştı, görsel hazırlama alanından çok uzaktı.
      - Kütüphane büyüyünce hepsi mi gösterilecek?
      - Kullanıcı ayrıca alt menü mü, aynı sayfada iç içe mi olacağını sordu.
    - **Karar:** Sayfa içi sekmeler (Işıklar sayfasındaki gibi), kenar çubuğunda alt menü değil. Uygulamada tek bir düzen kalıyor ve kenar çubuğu kısa kalıyor. Sekmeler:
      - **Görseller:** "Ekranda ne olsun?" ve hemen altında kütüphane.
      - **Canlı ekran:** canlı mod, bütçe, kaynaklar.
      - **Odak zamanlayıcısı.**
    - **Davranış:**
      - Son açılan sekme hatırlanır (`ks.screenTab`).
      - "Klavyede şu an" (ana sayfa ve GIF sayfası önizlemesi) Görseller ve Canlı ekran sekmelerinde üstte durur. Yükleme sayaçları başlıktan buraya taşındı.
      - Görseller sekmesi başka sekmeye geçince de bellekte kalır; yarım kalmış bir görsel kaybolmaz.
      - Kenar çubuğundaki süre ve Canlı ekrandaki zamanlayıcı satırı doğrudan Odak zamanlayıcısı sekmesini açar.
    - **Odak zamanlayıcısı tek kart:**
      - Üstte zamanlayıcının gösterdikleri: kalan süre halkası ve yanında klavye önizlemesi, altında düğmeler ve "Klavyede dene".
      - Altta "Ayarlar" listesi: Süreler, Tuş ışıkları, Faz başında ekran kartı, Faz bitince ses.
      - Tuş ışıklarının kendi ayarları (hareket, renkler, parlaklık, faz başında, nabız) o satırın altında girintili ve sol çizgili duruyor. Işıklar kapalıyken önizleme gri.
      - Canlı mod kapalıysa ekran kartı satırında "Canlı ekrana git" bağlantısı var.
    - **Kütüphane:**
      - Önce klavyede o an gösterilen görsel gelir, sonra fabrika animasyonu, sonra en yeniler.
      - İlk 6 görsel gösterilir; "Daha fazla göster" 12'şer ekler ve kaç tane kaldığını yazar. "Daha az göster" de var.
      - 6'dan fazla görsel varsa Tümü / Durağan / Hareketli süzgeci, 12'den fazlaysa arama kutusu çıkar.
      - Neden sayfa sayfa: her kart kendi resmini yüklüyor; yüzlerce görseli birden göstermek sayfayı ağırlaştırırdı.
    - **Doğrulama:**
      - Tarayıcıda sahte arka uç ve 15 deneme görseliyle: sayfa sayfa gösterme, süzgeç, arama ve "eşleşen yok" durumu; sekme hafızası; Canlı ekrandan Odak zamanlayıcısına bağlantı; yarım kart sekme değişince korunuyor.
      - Gerçek uygulamada (CDP) üç sekme ve kullanıcının kütüphanesi.

30. **Odak zamanlayıcısı kendi sayfasında (Araçlar).**
    - **Kullanıcının sorusu (2026-11-10):** Odak zamanlayıcısı hem Ekran'ın ayrı bir sekmesindeydi hem de Canlı ekran'ın kaynak listesinde bir satırdı; ikisinde de aynı "ekran kartı" anahtarı vardı. "Doğru yerde mi?"
    - **Karar:** Hayır.
      - Işık tasarımından sonra zamanlayıcının asıl çıktısı bütün klavyenin ışığı; ekran kartı bir seçenek ve canlı mod olmadan da çalışıyor. Onu Ekran'ın altında aramak doğal değil.
      - Aynı ayarın iki yerde olması da karışıklık yaratıyordu.
    - **Yeni düzen:**
      - Kenar çubuğunda yeni bir **Araçlar** grubu ve içinde **Odak zamanlayıcısı** sayfası. İleride gelecek canlı ışık araçları (CPU/GPU barı, ses düzeyi, mikrofon) da buraya.
      - Ekran sayfasında iki sekme kaldı: Görseller ve Canlı ekran. "Klavyede şu an" ikisinde de üstte. Eski kayıtlı `focus` sekmesi Görseller'e düşer.
      - Canlı ekran'ın kaynak listesindeki satır artık yalnız kaynağı gösteriyor: kartların açık mı kapalı mı olduğunu, zamanlayıcının durumunu ve "Ayarlarını aç" bağlantısını. Anahtar yok; ayarın tek yeri zamanlayıcının sayfası.
      - Zamanlayıcı sayfasında canlı mod kapalıyken "Canlı ekrana git" Ekran sayfasını Canlı ekran sekmesinde açar.
      - Kenar çubuğundaki kalan süre kutusu zamanlayıcının kendi sayfasında gizlenir (aynı bilgi sayfada var). Kutuya tıklayınca zamanlayıcı sayfası açılır.
    - **Doğrulama:** Tarayıcıda sahte arka uçla bütün bağlantılar ve kenar çubuğu. Gerçek uygulamada (CDP) zamanlayıcı sayfası, Ekran'ın iki sekmesi ve kaynak satırı.

## 4. Sistem nasıl kurulu (as-built)

```
crates/
  kb-devices/   Model veri dosyaları (models/*.toml): kimlikler, yetenekler, ölçülmüş geometri, LED indeksleri ve
                firmware göstergeleri (Num Lock, Fn). G/Ç yok.
  kb-protocol/  Saf paket kodlama. sonix.rs: başlık/yankı, guard (izin listesi ve bootloader engeli),
                efekt kataloğu, ışık paketi, tuş başına tablo, gerçek zamanlı tablo, ölçülmüş parlaklık
                ölçeği, F5 kırmızı baytının iki adayı. sonix/keymap.rs: tuş atama girişleri, katman tablosu,
                HID ve medya kataloğu. WASM'a derlenebilir.
  kb-transport/ hidapi (feature/output/input raporları) ve WinRT BLE (yalnızca okuma: pil).
  kb-core/      detect.rs (arayüzleri klavyelere gruplar), sonix.rs (sürücü: akışlar, zamanlama, yeniden deneme,
                sağlama toplamı), color.rs (sRGB→LED gamma/kazanç, ters dönüşüm, doğrusal parlaklık),
                readback.rs (F5 okumasında kırmızıyı beklenen renge göre çözer), clock.rs (yerel saat → klavye saati),
                keymap.rs (katmanı modele göre denetler: Fn kilidi, bilinmeyen tuş, kanonik biçim).
  kb-cli/       `kb` geliştirici aracı (bkz. §5).
app/
  src-tauri/    İnce kabuk. device.rs: TEK cihaz aktörü (HID'e dokunan tek yer; istek kuyruğu, canlı akış
                20 Hz, ayna 4 Hz, 2 sn yoklama). dto.rs: ts-rs ile TS tiplerine dönüşen DTO'lar. lib.rs: Tauri komutları.
                settings.rs: Rust'ın sahip olduğu kalıcı ayarlar (calibration.json, clock.json, keymap.json,
                macros.json; uygulama yapılandırma klasörü). keymap.json ve macros.json, klavyeye son yazılan
                atamanın ve makroların tek kaydıdır. screen_lib.rs: ekran kütüphanesi (screen/library.json,
                library/<id>.bin, live.bin). live.rs: canlı mod, yükleme bütçesi, bekleyen kart işçisi.
                now_playing.rs: Windows medya oturumlarından "Çalan şarkı".
                pomodoro.rs: Pomodoro zamanlayıcısı, tuş ışığı katmanı, faz kartı isteği.
  src/          React 19 + TS 7 + Tailwind 4 + motion. lib/ipc.ts (tipli köprü, seri kuyruk), lib/lighting.ts
                (Işıklar durumu), lib/mirror.ts, components/KeyboardView.tsx (geometriden çizim, boyama),
                components/CalibrationCard.tsx, components/UnsavedDialog.tsx, lib/unsaved.ts (kaydedilmemiş
                değişiklik listesi), components/ClockCard.tsx, lib/keymap.ts (atama taslağı ve kayıt),
                lib/keymapLabels.tsx (tuş üstü ve uzun adlar), lib/macros.ts (makro taslağı ve kayıt),
                lib/hidCodes.ts (e.code ↔ HID), lib/layout.ts (Windows düzenine göre tuş adları),
                lib/screen.ts (ekran kütüphanesi ve yükleme durumu), lib/screenImport.ts (dosya → RGB565),
                lib/screenText.ts (yazı kartı çizicisi), pages/{Overview,Lighting,Keymap,Macros,Screen,Settings}.tsx,
                lib/screenSlides.ts (slayt kare bütçesi ve geçişler), lib/screenPalette.ts (ekrana uygun ışık),
                lib/live.ts (canlı mod durumu ve kaynaklar), lib/liveCards.ts (canlı kartlar),
                lib/pomodoro.ts (Pomodoro durumu, sahte arka uç),
                pages/screen/ (Ekran Stüdyosu: common.tsx ortak adımlar, FileFlow.tsx, TextFlow.tsx, SlideFlow.tsx,
                MatchLights.tsx, LivePanel.tsx, PomodoroPanel.tsx).
  src/bindings/ `cargo test` ile ÜRETİLİR (TS tipleri, geometri JSON'u, efekt kataloğu). Elle düzenlenmez.
tools/cdp.mjs   Çalışan uygulamanın WebView2'sine CDP ile JS çalıştırır ya da ekran görüntüsü alır (uçtan uca test).
.github/workflows/ci.yml   PR ve birleştirmelerde Windows'ta kontroller (bkz. gelistirme-akisi.md).
rustfmt.toml, .gitattributes   120 sütun biçim; LF satır sonu.
```

**Güvenlik kuralları** (kodda zorunlu):
1. **Bekçi:** Her paket `kb_protocol::sonix::guard`'dan geçer. Bootloader dizileri, SN32 ISP desenleri ve izin listesi dışındaki opcode'lar engellenir.
2. **BLE:** Hiç yazma API'si yok. `FEE0/FEE1` (OTA) dokunulmaz.
3. **Flash'a yazan komutlar** (`04 13/23/11/27/15/17/72`) yalnızca açık kullanıcı eylemiyle gönderilir; kaydırıcı sürüklenirken asla.
4. **Gerçek zamanlı kare:** Başlık gönderildiyse kare her koşulda tamamlanır.
5. **Fn tuşu** yeniden atanamaz ve hedef olarak seçilemez (`kb-protocol` kodlayıcısı ve `kb_core::keymap::canonical`).

## 5. Nasıl çalıştırılır ve test edilir

```bash
cargo test --workspace            # 121 test; TS tiplerini ve mock JSON'ları da üretir
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check        # rustfmt.toml: 120 sütun
cd app && pnpm typecheck
```

```bash
# Uygulama (geliştirme): Vite sunucusu ve debug exe (debug exe devUrl'den yükler)
pnpm --dir app dev                 # ya da Claude için .claude/launch.json "ui"
cargo build -p klavye-studio && ./target/debug/klavye-studio.exe
```

```bash
# CLI (klavye USB modunda, uygulama KAPALI ya da aynası kapalıyken; aksi halde komutlar çakışır)
kb list | kb hid-all | kb descriptors | kb battery | kb models | kb effects
kb peek                                    # pasif okuma
kb read-colors [--repeat N --interval ms]  # 04 F5
kb live --pattern wave|solid|test|gamma    # 04 20, yalnızca RAM
kb live-probe                              # 04 20 + 04 F5: canlı katman, eksik tuşlar, kırmızı baytı (yalnızca RAM)
kb effect --mode 11 --rainbow --yes        # FLASH
kb per-key w=FF8A3D a=FF8A3D --yes         # FLASH
kb clock [--at "2026-11-10 12:34:56"]      # 04 28: klavye saati (RTC); ekran saat sayfasına geçer
kb direction-probe [--modes 10,11 --dirs 0,1 --speed 1] --yes   # FLASH: efekt yönlerini F5 ile ölçer
kb keymap-targets                          # atanabilecek tuş, medya ve fare adları
kb keymap --layer base kp-slash=a kp-star=media:volume-up caps=ctrl+z --yes   # FLASH: katmanın TAMAMI yazılır
kb keymap --layer fn --yes                 # FLASH: Fn katmanını fabrika ayarına döndürür
kb macros "text:ok" - "steps:click:right,wait:200,tap:x" --yes   # FLASH: BÜTÜN makrolar (yuva 0, 1 boş, 2)
kb macros --yes                            # FLASH: bütün makroları siler
kb screen-test --preview e1.png            # ekrana gidecek test desenini PNG olarak yazar (klavyeye dokunmaz)
kb screen-test --yes                       # FLASH: GIF sayfasına test deseni (16 sayfa, ~2,7 s)
kb screen-gif "%APPDATA%/dev.klavye.studio/screen/factory/ak980-pro-factory.gif" --yes   # FLASH: fabrika animasyonunu geri yükler (~5 dk)
kb keymap --layer base kp-slash=macro:0 kp-star=macro:0:times:3 --yes   # makroyu tuşa bağla
```

**Gerçek uygulamada uçtan uca test** (tarayıcı panelinde değil):

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/debug/klavye-studio.exe
node tools/cdp.mjs "document.body.innerText"   # ya da "@dosya.js"
node tools/cdp.mjs --shot ekran.png            # pencerenin ekran görüntüsü
```

Bu yöntemle düğmeleri tıklayıp ayna ve renk değerlerini okuyabilir, Rust log'unu (`RUST_LOG=klavye_studio=debug`) karşılaştırabilirsin. Hata ayıklama portunu kullanıcının normal çalıştırmasında açık bırakma.

**Klavyeye yazan testler:** Kullanıcının kayıtlı tasarımını ve uygulama durumunu (`localStorage` içindeki `ks.lighting.v1`, `ks.keymap.v1`) önce yedekle, testten sonra geri yükle. Yalnızca gereken kadar flash yazımı yap.
- **Tuş atama** klavyeden okunamaz: yazmadan önce kullanıcıya mevcut atamasını sor. Testten sonra katmanı fabrika ayarına döndür. CLI yazımı uygulamanın `keymap.json` kaydına girmez; uygulamayla çalışırken atamayı uygulamadan yaz.
- Kullanıcıdan tuşa basmasını isterken tuşu belirsizliksiz adlandır ("sayı tuş takımındaki / tuşu, Num Lock'a dokunmadan"). Uygulamadaki tuş testi kutusu ya da zaman damgalı bir dinleyici, kullanıcının kendi basışlarını ayırmaya yarar.

**İş akışı:** Her değişiklik `develop`'tan açılan bir dalda yapılır ve PR ile birleşir. CI yeşil olmadan birleştirilmez. Ayrıntı: `docs/gelistirme-akisi.md`.

## 6. Yaşanan sorunlar ve dersler (aynı hatayı tekrar etmemek için)

| # | Sorun | Kök neden | Ders |
|---|---|---|---|
| 1 | `rustc` çalışmıyordu | rustup toolchain'inin `bin` klasörü silinmişti | `rustup toolchain uninstall/install stable`. Bileşen ekleyerek onarmak işe yaramıyor. |
| 2 | USB modu 1 sn görünüp kayboldu | Anahtar USB konumundan geçip durmadı | Anahtar **ortada** = USB (soldan sağa 2.4G · USB · BT). PnP log'u (Kernel-PnP 400/411) teşhis eder. |
| 3 | bleak bağlı klavyeyi bulamadı | Bağlı cihaz reklam yapmıyor | WinRT `FromBluetoothAddressAsync` ile adresten bağlan. |
| 4 | `04 20` "reddedildi" sanıldı | Yankı durumu **00**, 01 değil | Yalnızca `FF` ret demek. Kaynaklardaki "01 bekle" kuralı bu cihazda geçersiz. |
| 5 | Sonraki komutlar yutuldu | Başlık gönderilip 8 veri paketi gönderilmeden çıkıldı | Veri duyuran başlıktan sonra akış **her koşulda** tamamlanır. |
| 6 | **Uygulamada ayna, canlı önizleme ve uygula çalışmadı** (CLI'de çalışıyordu) | `04 F5` sayacı `00` (S1/S2 biçimi): bir bağlantıda yalnızca ilk okuma çalışıyor, oturum kapanmıyor, sonraki komutlar yutuluyor | Sayaç **09** olmalı (S4/S8). Tekrarlanan işlemleri **aynı bağlantıda arka arkaya** test et; tek seferlik CLI testi bu hatayı gizler. |
| 7 | Ayna kapalı kaldı | Tauri IPC çağrıları paralel; React StrictMode'un aç/kapat/aç dizisi ters sırayla ulaştı | Durum değiştiren IPC'ler `ipc.ts` içindeki seri kuyruktan geçer. |
| 8 | Ara sıra `0x1F` HID hatası | GET_FEATURE stall | 5, 10 ve 15 ms bekleyerek 3 yeniden deneme. |
| 9 | Okuma kapanışı işlenmedi | `04 02` beklemesiz gönderildi | Resmi sürücünün 35 ms beklemesine uy. |
| 10 | Test sahtesi yanlış başarı verdi | Sahte, yankıları kuyrukta tutuyordu | Sahte, gerçek davranışı taklit etmeli: feature raporu **yalnızca son** yankıyı tutar. |
| 11 | Ekran görüntüleri eski kareyi gösterdi; sayfa geçişi tamamlanmadı | Tarayıcı paneli gizliyken çizim ve `requestAnimationFrame` durur; motion'ın çıkış animasyonu bitmediği için sayfa değişmez | Playwright (başsız) ya da gerçek uygulama için CDP kullan. Not: Playwright MCP yalnızca kendi çıktı klasörüne yazabiliyor; ekran görüntüsünü dosya adı vermeden al. |
| 12 | Palet renkleri soluk | Gamma: monitör sRGB, LED doğrusal | Renk testini **ara tonlarla** yap; saf renkler gamma'yı göstermez. Hipotezleri ayıran test tasarla (kanal sırası mı gamma mı?). |
| 13 | Kaynaklar çelişti (mod numaraları, kuyruk sırası, F5 sayacı, F5'in animasyonu izleyip izlemediği) | Farklı kart ve ürün yazılımı sürümleri | Ölçüm kaynaktan üstündür; ölçümü `donanim-dogrulama.md`'ye tarihli yaz. |
| 14 | "CLI'de çalışıyor" ama uygulamada çalışmıyor | Uygulama farklı zamanlama ve sıklıkla çağırıyor | Bir özelliği "bitti" saymadan önce **gerçek uygulamada** CDP ile uçtan uca dene. |
| 15 | Yeni bileşenin Tailwind sınıfları (ör. `grid-cols-[1fr_188px]`) uygulanmadı | Uzun süre açık kalan Vite sunucusu yeni dosyayı taramadı; sayfayı yenilemek yetmedi | Yeni dosya eklendikten sonra stil eksikse Vite'ı yeniden başlat. Hesaplanan stili CDP ile kontrol et (`getComputedStyle`). |
| 16 | `cargo fmt` tüm depoyu yeniden biçimlendirdi | Depoda rustfmt yapılandırması yoktu; kod 120 sütunla yazılmıştı | `rustfmt.toml` eklendi. Biçimlendirmeyi yalnızca yapılandırmayla çalıştır. |
| 17 | Bash aracında heredoc'la yazılan Python betiği bozuldu | İçerikteki tek sayıda kesme işareti (`sRGB'dir`) aracın tırnak işlemesini bozdu | Türkçe metin içeren betikleri önce Write ile scratchpad'e yaz, sonra çalıştır. |
| 18 | Canlı önizleme düşük parlaklıkta kayıtlı tasarımdan koyu görünüyordu | Parlaklık sRGB'de, gamma'dan önce uygulanıyordu | Parlaklık doğrusal büyüklüktür; kalibrasyondan **sonra** uygula (`apply_dimmed`). |
| 19 | **Temizle klavyeyi söndürmedi; boyanmayan tuşlar eski renkte kaldı** | Firmware girişleri idx baytına göre uyguluyor; boş yuva hiçbir şey değiştirmiyor. CLI testleri her zaman bütün tuşları gönderdiği için hata gizlendi. | Kare ve tablo **bütün** LED'leri açıkça tanımlamalı. Test araçları uygulamanın gerçekten gönderdiği biçimi (seyrek kare) de denemeli. `kb live-probe` bu soruyu doğrudan ölçer. |
| 20 | Tuş, fırça değiştikten hemen sonra eski rengi aldı | Olay işleyicisi önceki render'ın kapanışındaki rengi kullandı | Kullanıcı eylemi anında gereken durumu depodan (`getState()`) oku. |
| 21 | CDP testinde sürükleyerek boyama çalışmadı | React `onPointerEnter`'ı yerel `pointerenter`'dan değil, bırakılan öğenin `pointerout` olayından türetiyor | Sürüklemeyi `pointerout` (relatedTarget = yeni tuş) ve `pointerover` çiftiyle taklit et. |
| 22 | Kısayol dinleyicisi `window`'dan gelen olayda çöktü | `e.target.closest` her hedefte yok | Hedefi `instanceof Element` ile denetle. |
| 23 | Kapatma uyarısı nasıl test edilir? | CDP pencerenin X düğmesine basamıyor | `(Get-Process klavye-studio).CloseMainWindow()` gerçek WM_CLOSE gönderir; tarayıcı tarafını CDP ile sür. |
| 24 | Bindings kontrolü içerik aynıyken "değişti" dedi | Windows'ta git dosyaları CRLF açıyor, ts-rs LF yazıyor | `.gitattributes` (`* text=auto eol=lf`). Kontrol `git status` yerine içerik farkına (`git diff --quiet`) ve commit'lenmemiş dosyalara bakıyor. |
| 25 | Çalışan uygulamanın arayüzü dal değişince değişti | Debug exe arayüzü Vite'tan yüklüyor; Vite o anki çalışma ağacını sunuyor | Kullanıcı uygulamayı kullanırken başka dala geçme; geçtiysen işi bitirince güncel dala dön. |
| 26 | Ayna mor rengi lacivert gösterdi; FF "FA" okununca "6 bit çözünürlük" sanıldı | `F5` kırmızıyı `2·r mod 256` bildiriyor. FF→FA tesadüfen "yakın" göründü. | Okumayı tek değerle değil, **birkaç ara tonla ve her kanal için ayrı** doğrula. "Yaklaşık tuttu" sistematik bir hatayı gizleyebilir. |
| 27 | Sekme değişince canlı önizleme durmadı | Bekleyen 40 ms'lik kare zamanlayıcısı bileşen kapandıktan sonra tetiklendi ve akışı yeniden başlattı | Kapanışta (unmount) zamanlayıcıları temizle. Sayfa geçiş animasyonu açılışı geciktirdiği için hızlı tıklama bunu tetikleyebilir. |
| 28 | Saat gönderimi "sağlama toplamı tutmadı" hatası verdi | Saat akışında `04 02` toplamı 0 bildiriyor; ışık akışlarındaki davranış genellenmişti | Her akışın yankısını ayrı ölç. Saat akışında veri paketinin birebir yankısı teslimatı kanıtlıyor. |
| 29 | Tuş testinde her basıştan önce `NumLock` göründü, Num − hiç gelmedi | Talimattaki "Num /" yazısı "Num Lock + /" diye okundu; kullanıcı Num Lock'u basılı tutarak bastı | Tuşu belirsizliksiz adlandır. Dinleyiciye zaman damgası ve `keyup` ekle: basılı tutulan tuş, aynı basışın parçası mı ayrı bir tuş mu, hemen görünür. |
| 30 | `keymap.json` yazıldı ama geri okunamadı | Ayar dosyaları `#[serde(flatten)]` sarmalayıcısından geçiyor; flatten sayısal map anahtarlarını (`BTreeMap<u8, _>`) okuyamıyor | Bu dosyalarda map anahtarı metin olmalı. Her yeni ayar dosyasına yaz-oku testi ekle. |
| 31 | CDP betiği yanlış düğmeye tıkladı | Klavye görselindeki "A" ve "Ctrl" tuşları da düğme; ayrıca `startsWith("A")` "Alt"ı buldu | Seçiciyi ilgili bölümle sınırla ve metni birebir eşleştir. Beklenmeyen sonuçta önce test betiğinden şüphelen, sonucu ekran görüntüsüyle doğrula. |
| 32 | Yeni açılış kodu (klavye düzeni okuma) çalışmadı gibi göründü | Vite'ın anlık yenilemesi (HMR) `App`'in açılış etkilerini yeniden çalıştırmadı | Açılışta çalışan kodu denerken sayfayı yeniden yükle (`location.reload()`); yedeklenen `window` değişkenleri de silinir, karşılaştırmayı kayıtlı değerle yap. |
| 33 | (incelemede yakalandı) Seçici her çağrıda yeni `Set` döndürüyordu | zustand + React 19 `useSyncExternalStore`: değişen anlık görüntü sonsuz yeniden çizime yol açar | Seçiciden yalnızca ilkel değer (sayı, metin) ya da depodaki nesnenin kendisini döndür; türetilmiş nesneyi bileşende kur. |
| 34 | "Kaydet" düğmesi iki anlama geliyordu | Türkçede "kaydet" hem kayıt almak hem saklamak | Kayıt düğmesi "Kayda başla / Kaydı durdur"; saklama "Klavyeye kaydet". Arayüz metinlerini bağlam içinde, yan yana duran düğmelerle birlikte oku. |
| 35 | "Işıkları uydur çalışmıyor" | Tuşlar yanıyordu; palete beyaz yazı giriyordu, ortalı kartta renk sırası rastgeleydi | "Çalışmıyor"un ne demek olduğunu önce kanıtla ayır: günlük, CLI ile donanım testi, sonra kullanıcıya kısa ve seçenekli bir soru. LED paletinde gerçek renk varken beyaz, gri ve siyahı ele. |
| 36 | Slaytta yanlış küçük resim | Slayt anahtarları modül düzeyindeki bir sayaçtan geliyordu; HMR sayacı sıfırladı, anahtarlar çakıştı | Önbellek anahtarı olan kimlikleri bileşen başına (`useRef`) üret. Konsoldaki "same key" uyarısı bunun işaretidir. |
| 37 | Canlı kart yüklenince GIF önizlemesi yenilenmedi | "İlk durum" `null` ile ayırt ediliyordu; ilk gerçek değişiklik de `null`'dan geliyordu | "Henüz bilinmiyor" için `undefined`, "yok" için `null` kullan. |
| 38 | CI'da yalnızca rustfmt kırıldı | Push'tan önce `cargo fmt --check` çalıştırılmadı | Push'tan önce CI'ın adımlarını yerelde çalıştır: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm --dir app build`. |
| 39 | Uygulama penceresi boş kaldı ("crash oldu herhalde") | Uygulama yeniden başlatıldıktan hemen sonra `git stash` + `switch` yapıldı; arayüz Vite'tan yüklenirken dosyalar geçici olarak eksikti | Önce dalı hazırla, uygulamayı en son başlat. Başlattıktan sonra çalışma ağacını değiştirme; şüphede CDP ile sayfanın çizildiğini doğrula (bkz. #25). |
| 40 | `npx eslint` bir paketi indirip çalıştırdı | Projede ESLint (ve Prettier) yok; `npx` eksik aracı sormadan indiriyor | Bir aracı çalıştırmadan önce `app/node_modules/.bin`'de olduğuna bak ya da `npx --no-install` kullan. Projenin denetimleri: `tsc` (`pnpm build`), rustfmt, clippy. İndirilen paket önbellekten silindi. |
| 41 | Türkçe arayüze "Pomodoro" ve "flash" girdi | Tekniğin yabancı adı ve teknik terim, Türkçe metne olduğu gibi yazıldı | Türkçe sayfada yalnız Türkçe içerik: yabancı adın Türkçe karşılığını kullan ("Odak zamanlayıcısı", "kalıcı bellek"). Emin değilsen TDK sözlüğüne bak (`sozluk.gov.tr/gts?ara=…`). Yabancı ad gerekiyorsa yalnız belgelerde, parantez içinde. |
| 42 | Uygulamadaki odak önizlemesinde renk neredeyse görünmedi | Klavye çizimi ışığı gerçekteki gibi tuşun altından hafif bir parlama olarak gösteriyor; küçük boyutta bu sönük kalıyor | Amaç bir hareketi anlatmaksa tuşları düz renkle çiz (sohbetteki önizlemenin onaylanan hâli). Gerçekçi çizim, ışığın tasarımına bakılan yerde kalsın. |
| 43 | Tarayıcı testinde eklenen deneme görselleri sayfada görünmedi | `import('/src/lib/screen.ts')` uygulamanın kullandığı modül değildi: oturumda değişen dosyaları Vite `?t=…` ile yüklüyor, düz adres ayrı bir kopya açıyor | Test için modül alırken uygulamanın yüklediği adresi kullan: `performance.getEntriesByType('resource')` içinden bul. |

## 7. Kalan işler (öncelik sırasıyla)

### Sıradaki oturum için

1. `git switch develop && git pull`, sonra yeni iş için dal aç. PR açınca auto-merge ve auto-fix'i aç (`gelistirme-akisi.md`).
2. **Tuş atama ve makrolar bitti** (madde 19–21). Klavye fabrika durumunda: atama varsayılan, makro yok; `keymap.json` ve `macros.json` kayıtları da öyle.
3. **Ekran: Faz A ve B bitti, Faz C sürüyor.** Derin araştırma: `research/ekran-derin-arastirma.md`. **Pomodoro yapıldı (madde 28), kullanıcı denemesi bekliyor; sıradaki iş LED canlı katmanı** (aşağıda).
   - **Kullanıcının isteği:** "Ürünün farkı burada ortaya çıkacak." Ayrıca: "Buradaki görüntüyü de kaybetmeyelim" (stok ana sayfa) ve "birden fazla ekran görüntüsü veya başka ne yapabiliriz".
   - **Kısa sonuçlar:**
     - Resim ve GIF: evet, en çok 140 kare.
     - Video: canlı akış yok; kısa video en çok 140 karelik animasyona dönüşür.
     - Birden çok görsel: tek yuva, ama slayt (birkaç görsel tek animasyonda) ve PC'de kütüphane.
     - Stok ana sayfa (saat, tarih, pil) firmware'den çiziliyor ve yükleme ona dokunmuyor. Kaybolan tek şey GIF sayfasındaki fabrika animasyonu; resmi sürücünün `0.gif` dosyasıyla geri yüklenebilir. Bu bilgisayarda resmi sürücü kurulu değil.
     - Flash'a yazmadan ekran yolu stok firmware'de yok.
     - Kullanıcının "kendi katmanımız/firmware" fikri araştırma hattı olarak kaydedildi (§4.4).
   - **Yapıldı (2026-11-10):**
     - Fabrika animasyonu resmi sürücü paketinden yedeklendi; kullanıcı klavyedekiyle aynı olduğunu doğruladı.
     - Güvenli yükleme çekirdeği ve CLI (`kb screen-test`, `kb screen-gif`).
     - E1 (test deseni) ve E4 (fabrika animasyonu geri yüklendi) klavyede başarılı. Ayrıntı: `donanim-dogrulama.md`, "Ekran yüklemesi".
   - **Kullanıcının ilkesi (2026-11-10):** "Her ne yüklenirse yüklensin" dosya olduğu gibi gönderilmez. Uygulama onu klavyenin biçimine çevirir, önizletir, kullanıcı onaylarsa yükler. Ayrıntı: `research/ekran-derin-arastirma.md` §3.1.
   - **Sıradaki adımlar:**
     1. **Faz A bitti** (madde 23): Ekran Stüdyosu sayfası; kullanıcı kendi resim ve GIF'iyle denedi, E2 (kalıcılık) geçti.
     2. **Faz B bitti** (maddeler 24–26): kaynak seçici, yazı kartları, slayt, ışıkları ekrana uydurma. Kullanıcı klavyede denedi; "Işıkları uydur"un renkleri düzeltildi ve onaylandı.
     3. **Faz C sürüyor:** ilk adım bitti ve kullanıcı klavyede denedi (madde 27): canlı mod, bütçe, "Çalan şarkı", kapatınca önceki görseli geri yükleme.
        - **Pomodoro yapıldı** (madde 28). Kullanıcının klavyede denemesi bekliyor: ışıklar ve canlı mod açıkken faz kartları.
        - Sıradaki: LED canlı katmanı (CPU/GPU barı, ses düzeyi, mikrofon kapalı uyarısı), takvim, hatırlatıcı, oyun oturumu.
        - E3 (panelin gerçek kare hızı) hâlâ ölçülmedi. Saat paketinin flash'a yazıp yazmadığı da (odak kronometresi için).
        - Uygulama kapanınca ekranda son canlı kart kalıyor. Kapanışta da geri yükleme istenirse ayrıca tasarlanmalı: yükleme sürerken pencere kapanmamalı.
     4. Her yükleme `donanim-dogrulama.md`'ye yazılır. Bugüne kadar **13** yükleme: uygulamanın kaydında 11 (elle, canlı ve geri yükleme), CLI ile E1 ve E4.
   - **Faz B planı** (hepsi yapıldı, maddeler 24–26; plan kayıt için duruyor):
     - **Kullanıcının arayüz kuralı** (2026-11-10, Faz A geri bildirimi):
       - adımlar görünür olsun;
       - her adımda belirgin bir "Vazgeç" bulunsun;
       - iş bitince alan başlangıca dönüp ne yapıldığını söylesin;
       - amacı belli olmayan düğme olmasın.
       - Her yol (başarı, iptal, hata, geri dönüş) kullanıcıya göstermeden önce denenir: tarayıcı panelinde `http://127.0.0.1:1420/?mock=usb` (sahte arka uç, flash yazmaz) ve gerçek uygulamada CDP.
     - **Önce yeniden düzenleme** (`app/src/pages/Screen.tsx`):
       - Adım 1 bir kaynak seçici olacak, üç kutu: **Dosyadan** (bugünkü akış), **Yazı kartı**, **Slayt**.
       - Ortak bir çerçeve bileşeni (Stepper + Vazgeç) ve ortak bir önizleme/yükleme adımı (bugün `Editor` içinde: önizleme oynatıcısı, "Klavyeye yükle", "Düzenlemeye dön", "Yüklemeden kaydet", onay, satır içi ilerleme, bitince `onFinished`) ayrı bileşen olacak.
       - Her kaynak yalnızca adım 2'yi kendisi çizecek ve sonunda bir `Prepared` (kareler + gecikmeler) üretecek.
     - **B1, yazı ve şablon kartları:**
       - Şablonlar:
         - ad kartı (büyük ad, küçük alt yazı);
         - söz/alıntı;
         - kayan yazı (animasyon);
         - çerçevedeki dağlarla uyumlu çizgi-dağ teması;
         - rozet/emoji + yazı.
       - Ayarlar:
         - başlık ve alt yazı;
         - font: uygulamadaki Bricolage Grotesque, Instrument Sans ve JetBrains Mono (fontsource latin-ext, Türkçe karakterli);
         - boyut, yazı rengi;
         - arka plan: düz ya da iki renkli degrade;
         - hizalama;
         - kayan yazıda hız ve fps.
       - Kurallar:
         - Çizimden önce `document.fonts.load`.
         - Türkçe büyük harf için `toLocaleUpperCase("tr")`.
         - Yazıda yumuşak geçiş (dither) kapalı.
         - Kayan yazının kare sayısı ≤140 olacak şekilde adım otomatik büyütülür.
       - Canlı önizleme mevcut 2× tuvalde, RGB565 renkleriyle.
     - **B2, slayt:**
       - Slaytlar birden çok dosyadan ve kütüphaneden eklenir. Kütüphane öğeleri hazır 240×135; animasyonlu öğenin temsilci karesi alınır (`pickThumb`).
       - Slayt başına süre 1–20 s; dosya slaytlarında Doldur/Sığdır; sıralama (yukarı/aşağı), silme.
       - Geçiş: "Kesme" ya da "Yumuşak": 3 karelik çapraz geçiş, 100 ms.
       - Bütçe:
         - D saniyelik slayt = ⌈D / 0,508⌉ kare (her biri ≤508 ms).
         - Toplam ≤140 kare (≈71 s döngü).
         - Kare, döngü ve yükleme süresi göstergesi.
     - **B3, ekranla uyumlu ışık:**
       - Hazırlanan karelerden ya da bir kütüphane öğesinden 5 renklik palet çıkarılır (median cut).
       - "Işıkları ekrana uydur" iki seçenek sunar: tek renk (Static efekt) ya da tuşlara palet degradesi (tuş başına).
       - Seçim Işıklar sayfasında **taslak** olarak açılır: `useLighting` `setTab` / `setEffect` / `setPerKey`. Canlı önizleme tuşlarda (RAM) görünür; flash'a yazım yalnızca kullanıcı orada kaydedince olur, mevcut akış ve kapatma uyarısı korunur.
       - Not: kullanıcının kayıtlı ışığı şu an mod 0 (Kapalı).
     - **Sıra:** yeniden düzenleme → B1 → B2 → B3. Her biri ayrı PR olabilir. Donanım testi yalnızca kullanıcının onayıyla ya da kullanıcının kendi yüklemesiyle yapılır.
   - **Faz C planı** (bütçe, "Çalan şarkı" ve kapatınca geri yükleme yapıldı, madde 27; gerisi bekliyor). Not: "Çalan şarkı" bekleme süresi kullanıcının isteğiyle 4 sn; plandaki "Kapanışta iki seçenek" geri yükleme / kartı bırak olarak yapıldı. Ayrıntılı araştırma `research/ekran-ve-veri-kaynaklari.md`'de: tasarım §3.4, Windows veri kaynakları §5, plan "Faz 2".
     - **İlke:**
       - Ekran = olay tabanlı bağlam: bir sonraki olaya kadar doğru kalan içerik.
       - LED = gerçek zamanlı veri (`04 20`, RAM, flash'a yazmaz; 30–70 fps).
       - Saat = firmware'in ana sayfası.
       - Saat, pil yüzdesi, CPU yüzdesi gibi hızla eskiyen değerler görsele basılmaz.
     - **Yükleme bütçesi (her şeyden önce kurulur)**
       - Token bucket. Varsayılan "Dengeli": 24 yükleme/gün, anlık en çok 6. Ayrıca "Tasarruf" 8/gün ve "Yoğun" 48/gün (uyarılı).
       - Elle yapılan yüklemeler bütçeden düşer ama engellenmez. Sayaç zaten var (`library.json`: günlük ve toplam).
       - Tek bir "bekleyen kare" tutulur: en son gelen kazanır.
       - Aynı içerik (aynı içerik özeti) yeniden yüklenmez.
       - Bekleme kuralları kaynak başına (aşağıda).
       - Canlı kartlar yalnızca kullanıcı "Canlı mod"u açınca çalışır. Kapanışta iki seçenek: son görseli bırakmak ya da saat sayfasına dönmek (saat senkronuyla, yükleme gerekmez).
       - 2.4G ve Bluetooth'ta ekran güncellenemez: son durum kuyrukta bekler, kabloya dönünce bütçe izin verirse tek yükleme yapılır.
     - **Kartlar** (Faz B'nin şablon çizicisiyle çizilir; Türkçe fontlu):
       - **Çalan şarkı:**
         - Kaynak: Windows GSMTC (`GlobalSystemMediaTransportControlsSessionManager`): başlık, sanatçı, kapak.
         - Şarkı ≥10–15 s çaldıktan sonra yüklenir; duraklatılmışsa yüklenmez.
         - Seçenekler: her şarkı / albüm değişince / kısayolla sabitle.
       - **Pomodoro:** faz değişince "Odak 25:00" ya da "Mola" kartı; ilerleme LED barıyla.
       - **Takvim:** ICS'den "Sıradaki: 14:00 Standup". Göreli zaman ("5 dk sonra") yazılmaz, mutlak saat yazılır. Sıradaki etkinlik değişince yüklenir.
       - **Hatırlatıcı:** kısa kart. Sonra önceki görsele dönmek iki yükleme eder, bu yüzden bütçeye sayılır.
       - **Oyun oturumu (sonra):** ön plandaki uygulama algılanır; açılınca oyun kartı, kapanınca özet (süre, ortalama GPU, en yüksek sıcaklık).
     - **LED canlı katmanı** (flash'sız, sürekli):
       - CPU ve GPU barı rakam sırasında.
       - Ses düzeyi barı F sırasında, topuz çevrilince 1,5 s.
       - Ses spektrumu (WASAPI loopback, `cpal`).
       - Mikrofon kapalıyken bir tuş kırmızı.
       - Pomodoro ilerlemesi; bildirimde çift yanıp sönme.
       - Akış ana bilgisayarda birleştirilir. Uygulama kapanınca klavye kayıtlı efektine kendisi döner (doğrulandı).
     - **Deneysel "odak kronometresi":** Odak başında klavye saati 00:00:00'a ayarlanır; ana sayfa geçen süreyi yükleme yapmadan canlı gösterir (saniyeler zaten görünüyor). Bitince gerçek saat geri yüklenir. Yalnızca ileri sayar. Saat paketinin flash'a yazıp yazmadığı ölçülmeli.
     - **Arka plan:** Canlı mod uygulama açıkken çalışır. Tepsi simgesi ve Windows açılışında başlama, P2'deki "tuşla uygulama başlatma" ile birlikte tasarlanır (aynı arka plan servisi).
     - **Önce ölçülecekler:**
       - E3: panelin gerçek kare hızı.
       - Saat paketi flash'a yazıyor mu?
       - `04 20` sonrası kayıt yazmadan efekte dönüş (bilinen: akış durunca klavye kendi efektine dönüyor).
     - **Sıra önerisi:** bütçe altyapısı → çalan şarkı → Pomodoro (kart + LED) → LED canlı katmanı (CPU/GPU, ses) → takvim → hatırlatıcı → oyun oturumu.
   - **Güvenlik kuralları koda girecek** (§6):
     - kare ≤140, sayfa ≤2215, gecikme 1–254 (`FF` yasak);
     - `04 F0`, `04 38`, `1F`, güncelleme modu ve FF68'de GET_FEATURE asla gönderilmez;
     - sayfalar WriteFile ile ve her sayfanın onayı okunarak;
     - `04 02` toplamı doğrulanır.
4. Küçük açıklar:
   - Makroda "tekrar basana kadar" ve "basılı tuttukça" klavyede denenmedi.
   - Fare "ileri" yan tuşu, çift tık ve tekerlek ataması denenmedi.
   - **Seçenekler** (`04 17`) sonraya kaldı; bayt sırası kaynaklarda kesin değil.
5. Bekleyen küçük iş: "Kapalı" (mod 0) sonrası Fn+X ile ışığın geri açılıp açılmadığı. Kullanıcıyla, klavyede (P1). Not: kullanıcının kayıtlı ışığı şu an mod 0 (Kapalı).
6. **Kullanıcının kalibrasyonu:** Ayarlar'da gamma **1.0 (ham)** kayıtlı. Kullanıcı bunu bilerek seçti: 2.2'de "Gri basamaklar" deseninin sol tarafı soluk kalıyordu, bütün klavyenin yanmasını istedi.
   - **Açıklama (kullanıcıya iletildi):** Soldaki basamaklar desenin en koyu grileri (#202020, #404040); 2.2'de LED'de %1 ve %5'e düşüyorlar. Gamma parlaklığı değil ara tonları ayarlar. Beyaz ve saf renkler her gamma'da aynı yanar. 1.0'da ara tonlu renkler (mor, pembe, açık mavi) ekrandakinden daha soluk ve beyazımsı görünür. İlk göz testinde paletle en uyumlu değer 2.2 çıkmıştı.
   - **Karar kullanıcının:** Ayarını sormadan değiştirme.

### P0 — Hemen
- [x] Kullanıcı tuş başına renk akışını yeniden denedi: Fırça ve Seç araçları, kaydetme ve kapatma uyarısı.
- [x] Repo oluşturuldu: https://github.com/cebersaban44-rgb/klavye-studio (public; varsayılan dal `develop`, yayın dalı `main`). Commit'ler GitHub noreply e-postasıyla atılıyor (repo-yerel `user.email`).
- [x] Dal düzeni, PR akışı ve CI kuruldu (madde 14).
- [ ] Açık kaynak hazırlığı: lisans seçimi (henüz LICENSE yok, yani varsayılan olarak "tüm hakları saklı"). Geri kalanı P3'teki ilk sürüm gözden geçirmesinde.
- [x] Geliştirme log'larını sadeleştir: IPC `info` log'ları `debug`'a çekildi.
- [x] Kullanıcı Ayarlar → Renk kalibrasyonu'nu gözle denedi: gri basamaklar ve beyaz doğru.
- [x] Fn tuşunun kırmızı yanması şarj göstergesi: kullanıcı 2026-11-10'da gözledi (klavye şarj olurken, ışık kapalı modda bile). Model dosyasında `indicators` olarak kayıtlı.

### P1 — Işıklar tamamlama
- [x] Firmware efekt rengine (`04 13`) gamma gerekiyor mu? Hayır, firmware doğrusal. Uygulamanın kalibrasyonu efekt rengine de uygulanıyor (madde 15).
- [x] Ayna kırmızıyı doğru gösteriyor (`F5` kırmızıyı `2·r mod 256` bildiriyor; madde 15).
- [x] Ayarlar'a **renk kalibrasyonu**: kanal başına gamma ve kazanç, beyaz dengesi, klavyede test desenleri, kalıcı ayar.
- [x] Ayna ters kalibrasyonla gösteriyor.
- [x] Firmware'in parlaklık eğrisi: efektte ve tuş başına tabloda ölçüldü (`v·m[n] >> 10`, `m = [0, 191, 382, 573, 764, 1016]`). Canlı önizleme bu tabloyu kullanıyor.
- [x] Yön kodlarını doğrula: her yönlü efektte iki yön; arayüz hareketi adlandırıyor (madde 18).
- [ ] "Kapalı" (mod 0) sonrası Fn+X davranışını doğrula (kardeş klavyede ışık geri açılamıyordu). Kullanıcıyla, klavyede.
- [ ] Host tarafı canlı efektler (RAM, 20–40 fps): dalga, nefes, ses tepkili (WASAPI loopback, `cpal`), sistem metrikleri.
- [ ] Profiller (kaydet/yükle) ve uygulamaya göre otomatik profil (ön plandaki pencere).

### P2 — Diğer özellikler
- [x] **Saat senkronu** (`04 28`): bağlanınca günde en fazla bir kez ve elle (madde 17). Kullanıcı ekrandaki saatin doğru olduğunu onayladı.
- [ ] **Seçenekler** (`04 17`): uyku süresi, Win kilidi, Alt+Tab ve Alt+F4 kilidi, tepki süresi. Bayt sırası yakalamayla doğrulanmalı.
- [x] **Tuş atama** (`04 11` temel, `04 27` Fn): tuş, kısayol, medya, fare, tekerlek, devre dışı, varsayılana dönme; uygulamanın kendi kaydı (madde 19). Açık kalanlar:
  - [x] Makro bağlama (`06 SLOT MODE COUNT`): protokol, CLI ve Tuş Atama'daki "Makro" seçici (madde 20, 21).
  - [ ] Klavyede denenmedi: ileri yan tuş (`10`), çift tık (`03`), tekerlek (`01 03`). Kodlama kaynaklarla uyumlu; kullanıcıyla bir kez denenmeli.
  - [ ] `07 K1 K2 K3` (üç tuş birlikte) sunulmuyor; "Birlikte basılı" değiştiricileri çoğu ihtiyacı karşılıyor.
  - [ ] Kendi Fn işlevi olan tuşlarda Fn katmanı atamasının yok sayılıp sayılmadığı (kaynaklarda Fn+Home). Num / üzerinde Fn katmanı çalıştı.
  - [x] Atanacak tuşların adları Windows klavye düzenine göre (Keyboard Map API; madde 21). Klavye görselindeki tuş üstü yazılar modeldeki fiziksel etiketler olarak kaldı.
- [ ] **Tuşla uygulama başlatma** (kullanıcının fikri, 2026-11-10; açık iş, şimdilik yapılmıyor). Bir tuş ya da tuş kombinasyonu bir programı (`.exe`), dosyayı, klasörü ya da web adresini açsın.
  - **Neden yalnızca klavyeyle olmaz:** Klavye bilgisayarda program çalıştıramaz; tuş yalnızca bir kod gönderir. Programı, arka planda çalışan bir uygulama açar. Resmi sürücü de böyle yapıyor: `05 02 ID` girişi yalnızca resmi uygulama açıkken çalışıyor (`research/protokol-ff13.md` §6.2).
  - **İki yol:**
    1. **F13–F24 ile (önerilen):** Tuşa, klavyede fiziksel olarak bulunmayan F13–F24'ten biri atanır; bu bugünkü tuş atamayla zaten yapılabiliyor. Uygulama bu tuşları Windows'ta genel kısayol olarak dinler ve seçilen programı açar.
       - Temel katmanda 12, Fn katmanında 12 tuş daha. Ctrl/Shift/Alt ile birleşince çok daha fazla kombinasyon olur.
       - Klavyenin üreticiye özel kanalına bağlı değil. Atama klavyenin belleğinde durduğu için Bluetooth ve 2.4G'de de çalışması beklenir (doğrulanmalı).
       - Kullanıcı bu tuşları başka araçlarla da (PowerToys, AutoHotkey) kullanabilir.
    2. **Firmware'in kendi yolu (`05 02 ID`):** Tuş, FF13 kanalından bir bildirim (`05 xx ID`) gönderir; uygulama ID'ye bakıp programı açar. Yalnızca USB'de ve uygulama bu kanalı dinlerken çalışır. Hiç ölçülmedi.
  - **Gerekenler:**
    - Uygulamanın tepsi simgesiyle arka planda çalışması; isteğe bağlı olarak Windows açılışında başlaması.
    - Tuş Atama'da "Uygulama aç" kategorisi: program seçme, klasör ya da adres yazma; atamaların kalıcı kaydı.
    - Çakışma uyarısı: aynı F13–F24 kısayolunu başka bir program da kullanıyorsa.
  - **Güvenlik:** Yalnızca kullanıcının kendi seçtiği program, dosya ya da adres açılır. Yol ve argümanlar başka bir kaynaktan gelmez. Yönetici olarak çalıştırma yok.
  - **Önce ölçülecekler:** F13–F24'ün Windows'ta genel kısayol olarak yakalanması; atamanın BT ve 2.4G'de de geçerli olması.
  - **İlgili:** P1'deki "uygulamaya göre otomatik profil" de aynı arka plan servisini kullanır; ikisi birlikte tasarlanmalı.
- [x] **Makrolar** (`04 19` → `04 15`): protokol, sürücü, CLI, uygulamanın kaydı (`macros.json`), Makrolar sayfası ve Tuş Atama'da makro seçici (madde 20, 21). Açık kalanlar:
  - [ ] Oynatma biçimleri "tekrar basana kadar" ve "basılı tuttukça" klavyede denenmedi.
  - [ ] Metinden makro (yazı yaz) sunulmuyor: HID kodları konuma göre; düzene göre karakter → tuş eşlemesi gerekir (Keyboard Map API'nin tersi).
  - [ ] Kayıt sırasında fare tıklamaları yakalanmıyor (yalnızca "+ tık" düğmeleri). Windows tuşu ve Alt+Tab gibi sistem kısayolları tarayıcıda yakalanamıyor.
- [ ] **Ekran** (`04 72` + FF68). Faz A, Faz B ve Faz C'nin ilk adımı bitti, kullanıcı klavyede denedi (maddeler 22–27). Pomodoro yapıldı (madde 28). **Sıradaki: LED canlı katmanı** ("Sıradaki oturum için", madde 3).
  - Paketler **WriteFile** ile gönderilir, SET_REPORT ile değil (kardeş kartı çökertmiş). `04 F0` asla gönderilmez.
  - En fazla 140 kare ve 2215 sayfa; gecikme 1–254.
  - [x] Yüklenen görüntü ayrı, tam ekran GIF sayfasında görünüyor (E1).
  - [x] Ekran Stüdyosu: resim/GIF/video, yazı kartları, slayt, kütüphane, fabrika yedeği, ışıkları ekrana uydurma.
  - [x] Canlı mod: günlük bütçe (varsayılan 24), "Çalan şarkı", kapatınca önceki görsele dönüş.
  - [x] Pomodoro (madde 28; kullanıcı denemesi bekliyor).
  - [ ] Faz C'nin geri kalanı: LED canlı katmanı, takvim, hatırlatıcı, oyun oturumu.
- [ ] **Kendi firmware katmanımız** (kullanıcının fikri, 2026-11-10; araştırma hattı, şimdilik yapılmıyor). Stok firmware'de olmayan canlı ekranı (RAM'den çizim) kendi firmware'imizle ya da stok firmware'e yamayla eklemek.
  - Emsal: kardeş AK820 Pro'da QMK portu ve Jackrabbit firmware'i; panoyu firmware çiziyor, flash aşınması yok.
  - Proje kuralıyla çakışıyor: klavyeye firmware, bootloader ya da ISP komutu gönderilmez. Bu kuralı yalnızca kullanıcı kaldırabilir.
  - Ön koşullar: ayrılmış ikinci klavye, çip fotoğrafları, fabrika firmware kopyası, ISP ile geri dönüşün önce kanıtlanması.
  - Ayrıntı: `research/ekran-derin-arastirma.md` §4.4.
- [ ] **2.4G alıcı** ve Bluetooth: aşağıdaki "Sona bırakılan" bölümüne taşındı.

### Sona bırakılan — Bağlantı modlarına göre özellikler

Kullanıcıyla kararlaştırıldı (2026-11-10): Şu an her özellik yalnızca **USB**'de geliştiriliyor ve deneniyor. Bütün özellikler bitince **2.4G** ve **Bluetooth** modları için de tek tek kontrol edilip uygulanacak. O zamana kadar bu konuda kod yazılmaz; yalnızca bu liste güncel tutulur.

**Bugün bilinenler** (ayrıntı: `research/protokol-ff13.md` §1.1, §3, §11; ölçümler: `donanim-dogrulama.md`):

| Özellik | USB (FF13) | 2.4G alıcı (`0C45:FEFE`, MI_03 FF60, 33 bayt + toplam) | Bluetooth |
|---|---|---|---|
| Pil | Yok (USB'de pil kodu yok; `20 01` gönderilmez) | `20 01` (AK980 kaynaklarında doğrulanmış) | ✅ GATT 0x180F (ölçüldü) |
| Saat | ✅ `04 28` | `0C 10` (AK980 V2 yakalaması) | Bilinen yol yok |
| Firmware efekti | ✅ `04 13` | `05 10` (kardeş klavyelerde), flash | Yok |
| Tuş başına renk | ✅ `04 23` | `05 10 80` + `14` × 21 (yalnızca statik analiz) | Yok |
| Canlı ışık (RAM) | ✅ `04 20` | Per-key canlı kare **yok**; yalnızca `0B` müzik çubuğu | Yok |
| Ayna (`04 F5`) | ✅ | Bilinen okuma yok | Yok |
| Tuş atama | ✅ `04 11` / `04 27` | `10`/`12` (kardeşlerde; akış sınırları tehlikeli) | Yok |
| Makro | ✅ `04 19` → `04 15` (CLI) | `09` (kardeşlerde) | Yok |
| Seçenekler | Henüz yok | `07 10` (kardeşlerde) | Yok |
| Ekran | Henüz yok | `7F 03` (kardeşte, ~62 s) | Yok |

- **Bluetooth:** Ölçüm, üreticiye özel HID kanalı olmadığını gösterdi. Ayar yapılamıyor; `FEE0/FEE1` (OTA) asla yazılmaz. Uygulama BT'de pil ve "ayar için USB'ye geçin" yönlendirmesi gösteriyor.
- **2.4G:** Modelde `dongle` modu `lighting, clock, battery` olarak duruyor ama `verified = false`. Uygulamada dongle sürücüsü yok. Ölçüm için kullanıcının anahtarı 2.4G'ye alması ve alıcıyı takması gerekiyor.
- **Yapılacaklar (sırayla):**
  1. Kullanıcıyla 2.4G ölçümü: alıcının arayüzleri, `02 00` bilgi, `20 01` pil (yalnızca okuma).
  2. Dongle sürücüsü (`kb-core`): 33 baytlık çerçeve, toplam, eşleşen yankı, uykudan uyandırma için çift gönderim.
  3. Özellik özellik ölçüp açmak: saat, efekt, gerekirse diğerleri. Flash yazanları sınırla ve SEQ sınırlarına uy.
  4. Arayüz: her özelliğin hangi modda çalıştığını açıkça göster; modele göre kilitli/açık durumları doğrula.
  5. Model dosyasındaki mod yeteneklerini ölçüme göre güncelle (`verified`).

### P3 — Ürünleştirme
- [ ] Arka plan servisi:
  - Sistem tepsisi; pencere kapanınca WebView'ı yok et.
  - Otomatik başlatma, tek örnek (single-instance).
  - Takma/çıkarmayı `CM_Register_Notification` ile algıla.
- [ ] Release derleme (`custom-protocol`), NSIS kurulum, kod imzalama, imzalı güncelleyici.
- [x] CI (GitHub Actions, Windows): arayüz typecheck ve derleme, fmt, clippy, testler, bindings kontrolü; iki dalda birleştirme şartı. Ayrıntı: `docs/gelistirme-akisi.md`.
- [ ] Mock modda Playwright e2e testlerini CI'a ekle.
- [ ] Erişilebilirlik denetimi, TR/EN tamlığı, açık tema ince ayarı, uygulama adı ve logo kararı.
- [ ] **İlk sürümde belge ve GitHub gözden geçirmesi** (kullanıcıyla kararlaştırıldı: belgeler geliştirme bitince, ürünün son haline göre topluca düzenlenecek):
  - README: en üstte "Son sürümü indir" (Releases) bağlantısı, ekran görüntüleri, desteklenen modeller.
  - Lisans (LICENSE), `CONTRIBUTING.md` (PR'lar `develop`'a; kontroller), `SECURITY.md` (güvenlik açıkları özel kanaldan), davranış kuralları, kaynak teşekkürleri.
  - Issue şablonları: hata bildiriminde uygulama sürümü, Windows sürümü, klavye modeli ve firmware, bağlantı modu. Ayrıca PR şablonu.
  - **Dış katkılar için denetim** (kullanıcıyla kararlaştırıldı, 2026-11-10): Geliştirme döneminde PR'lar `develop`'a otomatik birleşiyor ve ikinci göz yok. Yayından sonra dışarıdan issue ve PR gelmeye başlayınca şunlar kurulacak:
    - Dışarıdan gelen PR'larda zorunlu inceleme ve CODEOWNERS; auto-merge yalnızca projenin kendi PR'larında.
    - İkinci göz: birleştirmeden önce bağımsız kod incelemesi (insan ya da yapay zekâ).
    - Issue ayıklama: etiketler, şablonlar, yinelenenlerin ve donanım testi isteyenlerin işaretlenmesi.
    - Fork'tan gelen PR'larda CI'ın gizli bilgilere erişmemesi.
  - Etiketler (bug, enhancement, needs-hardware-test, model:…) ve sürüm milestone'ları.
  - Bütün `docs/` belgelerinin güncelliği; `gelistirme-akisi.md` işleyişinin yeniden değerlendirilmesi.

### P4 — Çoklu model
- [ ] AK820 Pro (aynı PID 8009; ürün adıyla ayrılır), AK980 V2/MAX, AULA F108 Pro.
- [ ] Her biri bir model dosyası ve gerekirse bir protokol ailesi.

## 8. Açık sorular

- ~~Yüklenen ekran görüntüsünün yeri ve tek kare için gecikme baytı.~~ GIF sayfası (Fn+Home); tek kare için bayt 50 (100 ms) kullanılıyor ve çalışıyor.
- Ekran panelinin gerçek kare hızı (E3). Kayan yazı ve slayt şimdilik fabrika animasyonunun kanıtlanmış hızında (10 fps) varsayılıyor.
- Saat paketi (`04 28`) flash'a yazıyor mu? Odak kronometresi fikri buna bağlı.
- ~~Firmware efektleri rengi kendisi gamma'lıyor mu?~~ Hayır (madde 15).
- `F5`'te kaybolan kırmızı üst bit başka bir yerden okunabilir mi? Okunabilirse aynadaki son belirsizlik de kalkar.
- ~~`04 02` sağlama toplamı 16 bitte taşınca nasıl davranıyor?~~ Firmware 16 bitte sarıyor (97 beyaz tuşluk tabloyla doğrulandı).
- Fn tuşu (LED 96) neden her modda kırmızı? Num Lock (LED 32) beyazı Num Lock açıkken yanıyor.
- Yan ışık şeridi gerçekten var mı, yoksa yalnızca Fn+Z ile açılan logo ışığı mı? İpucu: Akış efektinde `F5`, tuşu olmayan yaklaşık 45 LED indeksini yanık döndürdü (`donanim-dogrulama.md`, "Efekt yönleri").
- Topuz yeniden atanabilir mi?
- Okuma opcode'ları (`04 10/12/16/22/26`): ayarları geri okumak için çok değerli, ama denenmeden önce risk değerlendirmesi ve yedek planı gerekli.
- Flash çipinin modeli (yazma ömrü hesabı için).

## 9. Ortam notları

- Windows 11, Rust 1.99.1 (stable, MSVC), Node 24, pnpm 10, TypeScript 7, WebView2 157.
- Uygulama adı geçici olarak "Klavye Stüdyo"; kimlik `dev.klavye.studio`.
- `.claude/launch.json` → "ui" (Vite, 127.0.0.1:1420).

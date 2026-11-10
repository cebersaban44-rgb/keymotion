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
| **Makrolar**: protokol, sürücü ve CLI (`kb macros`, tuşa `macro:N`) | ✅ Klavyede doğrulandı (madde 20). Uygulama sayfası henüz yok. |
| Makrolar ve Ekran sayfaları | ⏳ "Yapım aşamasında" yer tutucu |

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
    - **Uygulama sayfası henüz yok** (sıradaki iş).

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
                settings.rs: Rust'ın sahip olduğu kalıcı ayarlar (calibration.json, clock.json, keymap.json;
                uygulama yapılandırma klasörü). keymap.json, klavyeye son yazılan atamanın tek kaydıdır.
  src/          React 19 + TS 7 + Tailwind 4 + motion. lib/ipc.ts (tipli köprü, seri kuyruk), lib/lighting.ts
                (Işıklar durumu), lib/mirror.ts, components/KeyboardView.tsx (geometriden çizim, boyama),
                components/CalibrationCard.tsx, components/UnsavedDialog.tsx, lib/unsaved.ts (kaydedilmemiş
                değişiklik listesi), components/ClockCard.tsx, lib/keymap.ts (atama taslağı ve kayıt),
                lib/keymapLabels.tsx (tuş üstü ve uzun adlar), pages/{Overview,Lighting,Keymap,Settings,ComingSoon}.tsx.
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
cargo test --workspace            # 116 test; TS tiplerini ve mock JSON'ları da üretir
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

## 7. Kalan işler (öncelik sırasıyla)

### Sıradaki oturum için

1. `git switch develop && git pull`, sonra yeni iş için dal aç. PR açınca auto-merge ve auto-fix'i aç (`gelistirme-akisi.md`).
2. **Tuş atama bitti** (madde 19). **Makroların temeli bitti** (madde 20). Klavye fabrika durumunda: atama varsayılan, makro yok; `keymap.json` kaydı da öyle.
3. **Sıradaki iş: Makrolar sayfası** (kullanıcı makrolara başlamayı seçti).
   - **Kayıt:** Makrolar da klavyeden okunamaz. Uygulama kendi kaydını tutar (`macros.json`: yuva → ad ve adımlar). Cihaz aktörü klavye onayladıktan sonra günceller, `keymap.json` gibi.
   - **Düzenleyici:**
     - Makro listesi (ad, adım sayısı, süre).
     - Adım listesi: bas/bırak, tuş ya da fare, bekleme. Ekleme, silme, sıralama, beklemeyi düzenleme.
     - **Kaydet düğmesiyle kayıt:** `keydown`/`keyup`'ı `e.code` ile yakala (konuma göre). Böylece Türkçe Q düzeninde de doğru tuş yazılır.
     - Gerçek bekleme süreleri ya da sabit bekleme seçeneği.
     - Bellek göstergesi (`budget_used` / 0xA8C).
   - **Tuşa bağlama:** Tuş Atama'daki seçiciye "Makro" kategorisi; oynatma biçimi (bir kez, N kez, tekrar basana kadar, basılı tutulduğu sürece). Bağlı makro silinirken uyarı ve bağlantıyı kaldırma.
   - **Kaydetme:** Makrolar bütün görüntü olarak yazılır; tuş atamasından ayrı bir flash yazımı. Kaydedilmemiş makrolar kapatma uyarısına katılmalı.
   - **Diğer adaylar** (sonra): Seçenekler (`04 17`, yakalama gerekir), Ekran (`04 72` + FF68, en büyük ve riskli iş).
4. Bekleyen küçük iş: "Kapalı" (mod 0) sonrası Fn+X ile ışığın geri açılıp açılmadığı. Kullanıcıyla, klavyede (P1).
5. **Kullanıcının kalibrasyonu:** Ayarlar'da gamma **1.0 (ham)** kayıtlı. Kullanıcı bunu bilerek seçti: 2.2'de "Gri basamaklar" deseninin sol tarafı soluk kalıyordu, bütün klavyenin yanmasını istedi.
   - **Açıklama (kullanıcıya iletildi):** Soldaki basamaklar desenin en koyu grileri (#202020, #404040); 2.2'de LED'de %1 ve %5'e düşüyorlar. Gamma parlaklığı değil ara tonları ayarlar. Beyaz ve saf renkler her gamma'da aynı yanar. 1.0'da ara tonlu renkler (mor, pembe, açık mavi) ekrandakinden daha soluk ve beyazımsı görünür. İlk göz testinde paletle en uyumlu değer 2.2 çıkmıştı.
   - **Karar kullanıcının:** Ayarını sormadan değiştirme.

### P0 — Hemen
- [x] Kullanıcı tuş başına renk akışını yeniden denedi: Fırça ve Seç araçları, kaydetme ve kapatma uyarısı.
- [x] Repo oluşturuldu: https://github.com/cebersaban44-rgb/klavye-studio (public; varsayılan dal `develop`, yayın dalı `main`). Commit'ler GitHub noreply e-postasıyla atılıyor (repo-yerel `user.email`).
- [x] Dal düzeni, PR akışı ve CI kuruldu (madde 14).
- [ ] Açık kaynak hazırlığı: lisans seçimi (henüz LICENSE yok, yani varsayılan olarak "tüm hakları saklı"). Geri kalanı P3'teki ilk sürüm gözden geçirmesinde.
- [x] Geliştirme log'larını sadeleştir: IPC `info` log'ları `debug`'a çekildi.
- [x] Kullanıcı Ayarlar → Renk kalibrasyonu'nu gözle denedi: gri basamaklar ve beyaz doğru.
- [ ] Fn tuşunun sürekli kırmızı yanmasının nedenini öğren (hangi durumu gösteriyor?). İkisi de firmware göstergesi; model dosyasında `indicators` olarak kayıtlı.

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
  - [x] Makro bağlama (`06 SLOT MODE COUNT`): protokol ve CLI (madde 20). Tuş Atama sayfasında seçici henüz yok.
  - [ ] Klavyede denenmedi: ileri yan tuş (`10`), çift tık (`03`), tekerlek (`01 03`). Kodlama kaynaklarla uyumlu; kullanıcıyla bir kez denenmeli.
  - [ ] `07 K1 K2 K3` (üç tuş birlikte) sunulmuyor; "Birlikte basılı" değiştiricileri çoğu ihtiyacı karşılıyor.
  - [ ] Kendi Fn işlevi olan tuşlarda Fn katmanı atamasının yok sayılıp sayılmadığı (kaynaklarda Fn+Home). Num / üzerinde Fn katmanı çalıştı.
  - [ ] Tuş üstü adlar ABD düzenine göre (`;` `'` `[`); Türkçe Q düzeninde ekranda farklı karakter yazılır. İleride Windows klavye düzenine göre ad gösterilebilir.
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
- [ ] **Makrolar** (`04 19` → `04 15`): temel bitti (protokol, sürücü, CLI, klavyede doğrulandı; madde 20). Kalan: uygulamanın kaydı (`macros.json`), Makrolar sayfası, Tuş Atama'da makro seçici. `04 19` bütün makroları siler; her kayıt bütün görüntüyü yazar.
- [ ] **Ekran** (`04 72` + FF68):
  - Paketler **WriteFile** ile gönderilir, SET_REPORT ile değil (kardeş kartı çökertmiş). `04 F0` asla gönderilmez.
  - En fazla 140 kare ve 2215 sayfa.
  - Önce test deseniyle yüklenen 240×135 görüntünün **nerede** gösterildiğini netleştir (ana sayfa sağ paneli mi, Fn+Home ile açılan sayfa mı).
  - GIF stüdyosu; flash yazma bütçesi (günde 24 yükleme varsayılan) ve olay tabanlı kartlar.
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

- Yüklenen ekran görüntüsünün yeri ve tek kare için gecikme baytı (`00` mı `01` mi).
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

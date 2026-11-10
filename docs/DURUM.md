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
| Işıklar → **Efektler**: 20 firmware efekti, renk, gökkuşağı, parlaklık, hız, yön; "Klavyeye uygula" (flash) | ✅ (kullanıcı onayladı) |
| Işıklar → **Ayna**: klavyenin gerçek renklerini saniyede yaklaşık 4 kez `04 F5` ile okuyup gösterir | ✅ |
| Işıklar → **Tuş başına renk**: Fırça ve Seç araçları, hazır desenler, canlı önizleme (RAM), kalıcı kayıt, kayıt durumu ve "Kayıtlıya dön" | ✅ Eksik tuş hatası düzeltildi, akış uygulamada uçtan uca doğrulandı. Kullanıcının son denemesi bekleniyor. |
| Ayarlar: tema (sistem/koyu/açık), dil (TR/EN), Mica | ✅ |
| **Kaydedilmemiş değişiklik koruması**: kapatırken "Kaydet ve çık / Kaydetmeden çık / Vazgeç", kenar çubuğunda nokta | ✅ Uygulamada dört yol da denendi |
| Ayarlar → **Renk kalibrasyonu**: gamma (bağlı ya da kanal başına), beyaz dengesi, eğri grafiği, klavyede canlı test deseni (beyaz, gri basamaklar, palet), kalıcı ayar | ✅ Kullanıcı gri basamakları ve beyazı onayladı. |
| Genel Bakış → **Klavye saati**: bağlanınca otomatik eşitleme (günde en fazla bir kez, kapatılabilir), "Şimdi eşitle", son eşitleme zamanı | ✅ Uygulamada uçtan uca denendi; ekranın gözle kontrolü bekleniyor. |
| Tuş atama, Makrolar, Ekran sayfaları | ⏳ "Yapım aşamasında" yer tutucu |

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

## 4. Sistem nasıl kurulu (as-built)

```
crates/
  kb-devices/   Model veri dosyaları (models/*.toml): kimlikler, yetenekler, ölçülmüş geometri, LED indeksleri ve
                firmware göstergeleri (Num Lock, Fn). G/Ç yok.
  kb-protocol/  Saf paket kodlama. sonix.rs: başlık/yankı, guard (izin listesi ve bootloader engeli),
                efekt kataloğu, ışık paketi, tuş başına tablo, gerçek zamanlı tablo, ölçülmüş parlaklık
                ölçeği, F5 kırmızı baytının iki adayı. WASM'a derlenebilir.
  kb-transport/ hidapi (feature/output/input raporları) ve WinRT BLE (yalnızca okuma: pil).
  kb-core/      detect.rs (arayüzleri klavyelere gruplar), sonix.rs (sürücü: akışlar, zamanlama, yeniden deneme,
                sağlama toplamı), color.rs (sRGB→LED gamma/kazanç, ters dönüşüm, doğrusal parlaklık),
                readback.rs (F5 okumasında kırmızıyı beklenen renge göre çözer), clock.rs (yerel saat → klavye saati).
  kb-cli/       `kb` geliştirici aracı (bkz. §5).
app/
  src-tauri/    İnce kabuk. device.rs: TEK cihaz aktörü (HID'e dokunan tek yer; istek kuyruğu, canlı akış
                20 Hz, ayna 4 Hz, 2 sn yoklama). dto.rs: ts-rs ile TS tiplerine dönüşen DTO'lar. lib.rs: Tauri komutları.
                settings.rs: Rust'ın sahip olduğu kalıcı ayarlar (calibration.json, clock.json; uygulama yapılandırma klasörü).
  src/          React 19 + TS 7 + Tailwind 4 + motion. lib/ipc.ts (tipli köprü, seri kuyruk), lib/lighting.ts
                (Işıklar durumu), lib/mirror.ts, components/KeyboardView.tsx (geometriden çizim, boyama),
                components/CalibrationCard.tsx, components/UnsavedDialog.tsx, lib/unsaved.ts (kaydedilmemiş
                değişiklik listesi), components/ClockCard.tsx, pages/{Overview,Lighting,Settings,ComingSoon}.tsx.
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

## 5. Nasıl çalıştırılır ve test edilir

```bash
cargo test --workspace            # 76 test; TS tiplerini ve mock JSON'ları da üretir
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
```

**Gerçek uygulamada uçtan uca test** (tarayıcı panelinde değil):

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/debug/klavye-studio.exe
node tools/cdp.mjs "document.body.innerText"   # ya da "@dosya.js"
node tools/cdp.mjs --shot ekran.png            # pencerenin ekran görüntüsü
```

Bu yöntemle düğmeleri tıklayıp ayna ve renk değerlerini okuyabilir, Rust log'unu (`RUST_LOG=klavye_studio=debug`) karşılaştırabilirsin. Hata ayıklama portunu kullanıcının normal çalıştırmasında açık bırakma.

**Klavyeye yazan testler:** Kullanıcının kayıtlı tasarımını ve uygulama durumunu (`localStorage` içindeki `ks.lighting.v1`) önce yedekle, testten sonra geri yükle. Yalnızca gereken kadar flash yazımı yap.

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
| 11 | Ekran görüntüleri eski kareyi gösterdi | Tarayıcı paneli gizliyken çizim durur | Playwright (başsız) ya da gerçek uygulama için CDP kullan. Not: Playwright MCP yalnızca kendi çıktı klasörüne yazabiliyor; ekran görüntüsünü dosya adı vermeden al. |
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

## 7. Kalan işler (öncelik sırasıyla)

### Sıradaki oturum için

1. `git switch develop && git pull`, sonra yeni iş için dal aç (örneğin `feature/direction-codes`).
2. Önerilen sıra:
   - Yön kodları (P1).
   - Tuş atama (P2, büyük iş).
3. **Kullanıcının kalibrasyonu:** Ayarlar'da gamma **1.0 (ham)** kayıtlı. Kullanıcı bunu bilerek seçti: 2.2'de "Gri basamaklar" deseninin sol tarafı soluk kalıyordu, bütün klavyenin yanmasını istedi.
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
- [ ] Yön kodlarını doğrula (özellikle mod 10). "Kapalı" (mod 0) sonrası Fn+X davranışını doğrula.
- [ ] Host tarafı canlı efektler (RAM, 20–40 fps): dalga, nefes, ses tepkili (WASAPI loopback, `cpal`), sistem metrikleri.
- [ ] Profiller (kaydet/yükle) ve uygulamaya göre otomatik profil (ön plandaki pencere).

### P2 — Diğer özellikler
- [x] **Saat senkronu** (`04 28`): bağlanınca günde en fazla bir kez ve elle (madde 17). Ekranın gözle kontrolü kullanıcıda.
- [ ] **Seçenekler** (`04 17`): uyku süresi, Win kilidi, Alt+Tab ve Alt+F4 kilidi, tepki süresi. Bayt sırası yakalamayla doğrulanmalı.
- [ ] **Tuş atama** (`04 11` temel, `04 27` Fn; 9 paket, 576 bayt): HID kodları, medya, fare, makro bağlama; varsayılana dönme; yazmadan önce yedek.
- [ ] **Makrolar** (`04 19` → `04 15`): `04 19` makro tablosunu siler, dikkat.
- [ ] **Ekran** (`04 72` + FF68):
  - Paketler **WriteFile** ile gönderilir, SET_REPORT ile değil (kardeş kartı çökertmiş). `04 F0` asla gönderilmez.
  - En fazla 140 kare ve 2215 sayfa.
  - Önce test deseniyle yüklenen 240×135 görüntünün **nerede** gösterildiğini netleştir (ana sayfa sağ paneli mi, Fn+Home ile açılan sayfa mı).
  - GIF stüdyosu; flash yazma bütçesi (günde 24 yükleme varsayılan) ve olay tabanlı kartlar.
- [ ] **2.4G alıcı** (`0C45:FEFE`, FF60, 33 bayt ve sağlama toplamı): pil `20 01`, saat `0C 10`, ışık.

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
- Yan ışık şeridi gerçekten var mı, yoksa yalnızca Fn+Z ile açılan logo ışığı mı?
- Topuz yeniden atanabilir mi?
- Okuma opcode'ları (`04 10/12/16/22/26`): ayarları geri okumak için çok değerli, ama denenmeden önce risk değerlendirmesi ve yedek planı gerekli.
- Flash çipinin modeli (yazma ömrü hesabı için).

## 9. Ortam notları

- Windows 11, Rust 1.99.1 (stable, MSVC), Node 24, pnpm 10, TypeScript 7, WebView2 157.
- Uygulama adı geçici olarak "Klavye Stüdyo"; kimlik `dev.klavye.studio`.
- `.claude/launch.json` → "ui" (Vite, 127.0.0.1:1420).

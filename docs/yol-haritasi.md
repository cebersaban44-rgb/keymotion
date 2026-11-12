# Keymotion yol haritası: ilk sürüme kadar

Durum: 2026-11-12. Kaynaklar: kullanıcının kararları, `docs/DURUM.md` madde 38, `docs/urun-eksikleri.md`.

Bu belge kalan işlerin **sırasını** tutar. Her aşamanın ayrıntısı ve kararları da burada. Bir aşama bitince "Durum" sütununu ve DURUM'daki "Kalınan yer" satırını güncelle.

## Sıralama ilkeleri

1. **Önce uygulamanın temel davranışları, sonra paketleme.** Kurulum dosyası, Windows'la başlatma kaydı, kısayollar, imza ve güncelleyici; exe adına ve pencerenin yaşam döngüsüne bağlı. Bunlar sonradan değişirse kullanıcıların bilgisayarında eski kayıtlar kalır.
2. **Dışarıdan bekleme süresi olan işler erken başlar ve paralel yürür.** Kod imzalama başvurusu onay bekler; başvuru Aşama 1'le birlikte yapılır.
3. **Ekran görüntüleri ve belgeler en sona.** Arayüz kesinleşmeden çekilen görüntüler eskir.
4. **Klavyeye yazan her deneme önce kullanıcıya sorulur.** Ne yazılacağı kısaca söylenir.
5. **Her aşama aynı biçimde biter:**
   - denetimler: cargo fmt/clippy/test, tsc, ESLint, `tools/check-*.mjs`;
   - gerçek uygulamada doğrulama (CDP);
   - belgeler;
   - PR'ın `develop`'a birleşmesi.
6. Yardımcı ajanlar kendi worktree'lerinde çalışır. Başlattıkları sunucuları kapatır (ders 55). İşleri birleşince klasörleri silinir.

## Özet

| Sıra | Aşama | Bağımlılık | Kullanıcıdan gereken | Durum |
|---|---|---|---|---|
| 0 | Açık işleri kapat (lint/mock yardımcısı, PR #58) | — | — | Bitti (PR #58, #59) |
| 1 | Rust paket ve exe adı `keymotion` | 0 | Onay (öneri: evet) | Bitti (PR #60) |
| 1b | Kod imzalama başvurusu (SignPath Foundation) | — | Başvuruyu kullanıcı yapar | Başlatılacak |
| 2 | Tepsi ve arka plan | 1 | Karar verildi: tepsi | Bitti (PR #61): kullanıcı 1–8, ben 9–13 denedik |
| 3 | Keymotion açılış animasyonu | — (2 ile paralel olabilir) | Önizlemeye onay | Bekliyor |
| 4 | Sorun bildirme ve GitHub sorun formları | 2 | Özel güvenlik bildirimi ayarı için onay | Bekliyor |
| 5 | Kurulum dosyası (NSIS) ve imza | 1, 2, 1b | Kurulum ayrıntılarına onay | Bekliyor |
| 6 | Sürüm hattı ve otomatik güncelleyici | 5 | Güncelleme anahtarını GitHub Secrets'a koyma onayı | Bekliyor |
| 7 | GitHub sayfası ve belgeler (EN varsayılan + TR) | 2, 3, 4 (arayüz kesin) | — | Bekliyor |
| 8 | Son denemeler ve ilk sürüm 0.1.0 | hepsi | Elle denemeler; `main`'e birleştirme onayı | Bekliyor |

## Aşama 0: açık işleri kapat

- **Lint ve sahte arka uç yardımcısı:**
  - 15 ESLint uyarısı davranış değişmeden düzeltilecek.
  - ESLint CI'da engelleyici olacak (sıfır uyarı).
  - Ders 40 güncellenecek.
  - Sahte arka uç (`?mock`, `?fw`, `?reset`, `?failPart`) yalnızca geliştirme sürümünde kalacak.
  - Yardımcının raporu gelince her uyarı için yapılanı incele. Üretim derlemesinde sahte kodun kalmadığının kanıtına bak. Sonra birleştir.
- **PR #58:** repo adı, İngilizce varsayılan, uyarı günlüğü, belgeler, notices. Otomatik birleşmede.

## Aşama 1: Rust paket ve exe adı

- **Öneri:** `klavye-studio` → `keymotion`.
  - Kurulu exe `keymotion.exe` olur; Görev Yöneticisi'nde ve Windows'la başlatma kaydında bu ad görünür.
  - Kütüphane adı `keymotion_lib`. Paket ve kütüphane aynı adı taşırsa Windows'ta `.pdb` dosyaları çakışır.
- **Neden şimdi:** Windows'la başlatma kaydı (Aşama 2), kısayollar, imza (Aşama 5) ve güncelleyici (Aşama 6) exe adına bağlı.
- **Dokunulacak yerler:**
  - `app/src-tauri/Cargo.toml` (paket ve lib adı), `main.rs` (`klavye_studio::`);
  - `tauri.conf.json` (`mainBinaryName` gerekirse);
  - `README.md`, `docs/DURUM.md` §5 (`target/debug/klavye-studio.exe`), CI;
  - `tools/cdp.mjs` notları, yardımcı brifleri.
- **Değişmeyenler:** veri klasörleri kimliğe bağlı (`dev.keymotion`), taşıma gerekmez. `migrate.rs` dokunulmaz.
- **Doğrulama:** temiz derleme (`cargo clean -p` ile, ders 50), uygulama açılıyor, ayarlar yerinde.

## Aşama 1b: kod imzalama başvurusu (paralel, erken)

- **Neden:** imzasız bir kurulum dosyasını Windows SmartScreen "tanınmayan uygulama" diye engeller. İndirenler korkar.
- **Seçenek A (öneri): SignPath Foundation.** Açık kaynak projelere ücretsiz.
  - Şartlar: herkese açık repo ✓, OSI uyumlu lisans (MIT OR Apache-2.0) ✓, düzenli sürüm.
  - Başvuruyu kullanıcı yapar; formu birlikte doldururuz.
  - Onay süreci sürebilir. Bu yüzden başvuru şimdi.
- **Seçenek B: Azure Artifact Signing.** Ücretli; kişisel kimlik doğrulaması ister.
- **Onay gelene kadar:** Aşama 5–6 imzasız deneme sürümleriyle ilerler. İmza en son hatta eklenir.

## Aşama 2: tepsi ve arka plan

**Kullanıcının kararı (2026-11-12):** pencere kapatılınca uygulama tepside, yani saatin yanındaki simge alanında çalışmaya devam eder. Önerilen ayrıntılar kabul edildi.

### Neden

- **Klavyede kalanlar:** ışıklar, tuş atamaları, makrolar ve ekran görseli klavyenin kendi belleğinde. Uygulama kapalıyken de çalışırlar.
- **Uygulamaya bağlı olanlar:** bunlar uygulama çalışırken iş görür; bugün pencere kapanınca hepsi duruyor:
  - odak zamanlayıcısı;
  - hatırlatıcılar;
  - takvim kartları;
  - canlı ekran (saat, çalan şarkı);
  - tuşlardaki sistem göstergeleri;
  - klavye saatinin eşitlenmesi.

### Davranış

- **Kapatma:** X ya da Alt+F4 pencereyi gizler, uygulama tepside çalışır.
  - Ayar: "Kapatınca tepside çalışmaya devam et", varsayılan **açık**.
  - Kapalıysa bugünkü gibi tamamen kapanır.
- **İlk gizleme:** yalnızca bir kez Windows bildirimi çıkar: "Keymotion arka planda çalışıyor. Simgesi saatin yanında." Bildirim `notify.rs` ile gönderilir.
- **Tepsi simgesi:**
  - Sol tık pencereyi gösterir ve öne getirir.
  - Araç ipucu "Keymotion". Odak sürüyorsa kalan süre de görünür.
- **Sağ tık menüsü** (arayüz dilinde; metinler `tray_texts` ile arka uca iletilir). Kullanıcının ilk denemesinden sonra (2026-11-12) alt menülü düzene geçildi:
  - en üstte klavye satırı, tıklanmaz: "AJAZZ AK980 PRO · USB kablo", kablo yoksa pil yüzdesiyle; klavye yoksa "Klavye bağlı değil";
  - Keymotion'ı aç;
  - Odak zamanlayıcısı ▸
    - üstte, tıklanmaz: aşama, tur ve kalan dakika ("Odak · Tur 2/4 · 18 dk kaldı", "Duraklatıldı · Kısa mola · 3 dk kaldı", boştayken "Hazır: 4 × 25 dk odak");
    - Başlat / Duraklat / Sürdür (duruma göre);
    - Sonraki aşamaya geç;
    - Durdur.
    - Sonraki aşamaya geç ve Durdur yalnızca zamanlayıcı çalışırken ya da duraklatılmışken kullanılır.
  - Hatırlatıcılar ▸ sıradaki hatırlatıcı ("Sıradaki: 14:30 · başlık", "yarın 09:00"), "Hatırlatıcılar açık" işareti;
  - Tuşlarda sistem göstergeleri (işaretli/işaretsiz; yalnızca RAM, flash'a yazmaz);
  - Profiller ▸ kayıtlı profiller, klavyede olan işaretli;
  - Çık.
  - Menü bir kez kurulur, sonra yalnızca maddelerin yazısı ve işaretleri değişir. Bu yüzden açık menü kapanmaz, kalan dakika menü açıkken de güncellenir (10 sn'de bir).
  - **Profil seçimi** klavyeye yazar. Bu yüzden pencereyi açar ve Profiller sayfasında o profilin onay satırını gösterir. Onaysız yükleme yok; bu, "her adımda Vazgeç" kuralına uyar.
- **Çık:** kaydedilmemiş değişiklik ya da süren bir yazma varsa pencere açılır ve mevcut kapatma penceresi gösterilir (`useCloseGuard`, `UnsavedDialog`). Yoksa uygulama kapanır.
- **İkinci açılış:** gizli pencereyi gösterir. Tek örnek eklentisi zaten var; `show()` eklenir.
- **Windows kapanırken ya da oturum kapanırken:** normal çıkış. Yazma sürüyorsa mevcut `ShutdownBlockReasonCreate` devrede.

### Windows açılınca başlatma

- Ayar: "Windows açılınca başlat", varsayılan **kapalı**.
- Açılırsa `tauri-plugin-autostart` kullanılır (HKCU `Run` kaydı). Uygulama `--hidden` argümanıyla başlar ve pencere gösterilmez, yalnızca tepsi simgesi çıkar.
- Kaldırıcı bu kaydı siler (Aşama 5).

### Ayarlar

- Ayarlar sayfasında yeni "Arka plan" kartı, iki anahtarla:
  - "Kapatınca tepside çalışmaya devam et";
  - "Windows açılınca başlat".
- Kartta kısa bir açıklama: hangi özellikler arka planda çalışır.
- Ayarlar arka uçta saklanır (örneğin `app.json`, `settings.rs` kalıbıyla).

### Teknik riskler ve ölçümler

- **Kartlar gizli pencerede:** canlı kartlar WebView'da çiziliyor.
  - Arka uç olayları gönderir: `calendar-card`, `pomodoro-card`, çalan şarkı. Arayüz kartı çizer ve `live_submit` ile geri yollar.
  - Pencere gizliyken WebView2 sayfayı "görünmez" sayar: `requestAnimationFrame` durur, zamanlayıcılar kısılır. Olay dinleyicileri çalışır ama çizim yolu rAF ya da yazı tipi yüklemesi beklerse takılır.
  - **Ölç:** pencere gizliyken saat, odak ve takvim kartı zamanında yükleniyor mu? Günlükte `live` satırlarına bak.
  - **Gerekirse:** çizimi rAF'siz yap, ya da WebView2 argümanları ekle: `--disable-background-timer-throttling`, `--disable-renderer-backgrounding`, `--disable-backgrounding-occluded-windows`.
- **Bellek ve işlemci:** gizli WebView yaklaşık 150–250 MB bellekte kalır.
  - Ölç: `Get-Process`.
  - Ayna okumasını (`04 F5`) yalnızca Genel Bakış görünürken yap. Bugün sürekli okuyor ve ara sıra reddediliyor.
  - Odak, göstergeler ve hatırlatıcılar zaten arka uçta.
- **Güç kullanımı** burada ölçülür: boşta, odak sürerken, göstergeler açıkken. Windows "Güç kullanımı" ve işlemci zamanıyla.
- **Takma ve çıkarma:** bugün klavyeler düzenli taramayla bulunuyor. Arka planda güç için `CM_Register_Notification` bildirimine geçmeyi değerlendir (DURUM P3).

### Ölçümler (2026-11-12, debug sürüm)

- **Gizli pencerede kısılma yok.** Tauri pencereyi `SW_HIDE` ile gizliyor, WebView2'ye "görünmez" denmiyor:
  - `document.visibilityState` "visible" kalıyor;
  - 100 ms'lik zamanlayıcı 1,5 sn'de 15 kez çalıştı;
  - `requestAnimationFrame` çalıştı;
  - örnek hatırlatıcı kartı 137 ms'de çizildi.
  - Bu yüzden WebView2 argümanı eklenmedi.
  - **6 dakika 20 saniye gizli kaldıktan sonra** (Chromium'un 5 dakikalık yoğun kısma eşiğinin ötesi; senaryo 13) yine aynı: sayfa "visible", zamanlayıcı 1,5 sn'de 15 kez, `requestAnimationFrame` çalıştı, kart 457 ms'de çizildi.
- **Ayna okuması (`04 F5`) pencere gizliyken duruyor:** `useMirror` artık `windowShown`'a bağlı.
- **Bellek:** pencere gizliyken 7 süreç, toplam çalışma kümesi yaklaşık 270 MB (debug sürüm, hata ayıklama portu açık). Kurulu sürümde yeniden ölçülecek.
- **Bellek, 6 dk gizli kaldıktan sonra:** çalışma kümesi 170 MB (Windows boştaki sayfaları geri aldı), özel bellek 317 MB.
- **İşlemci:** 6 dk 20 sn gizliyken toplam 5,6 sn işlemci zamanı, yani tek çekirdeğin ortalama yaklaşık %1,5'i. Bu ölçüm debug sürümde, hata ayıklama portu açıkken alındı; kurulu sürümde Görev Yöneticisi › Güç kullanımı ile yeniden bakılacak.

### Doğrulama (gerçek uygulama)

**Yapılanlar (2026-11-12, CDP ile):**
- kapat → gizlen, ilk bildirim bir kez (`background.json` `noticeShown`);
- ikinci açılış pencereyi gösteriyor;
- gizli açılış (`--hidden`) ve sonra ikinci açılış;
- arayüz `windowShown`'ı izliyor;
- Ayarlar › Arka plan › "Windows açılınca başlat": `Run` kaydı `"…\keymotion.exe" --hidden` olarak yazıldı ve kapatınca silindi;
- tepsiden profil seçmenin olayı Profiller sayfasında onay satırını açtı, "Vazgeç" kapattı.

**Kullanıcının denediği (ilk menü):** odak başlat/duraklat çalıştı, ama bitirme yoktu. Bunun üzerine alt menülü düzene geçildi.

**Kullanıcının denediği (alt menülü sürüm):** aşağıdaki 1–8 tamam.
- Bildirim "Windows PowerShell" adı ve simgesiyle geldi. Düzeltildi, aşağıda "Bildirimler".
- "İkinci açılış İngilizce ve simgesiz" gözlemi: günlükte o saatte uygulama açılışı yok. Açılan, Claude uygulamasının önizleme panelindeki tarayıcı kopyasıydı (`ui`, 1420): arka ucu yok, İngilizce başlar, tepsisi yok (DURUM ders 57).

**Benim denediklerim (9–13, CDP ile, fare kullanmadan):**
- **9:** "Kapatınca tepside…" kapalıyken X uygulamayı tamamen kapattı.
- **10:** Windows'la başlatma:
  - açınca `Run` kaydı `--hidden` ile yazıldı;
  - Görev Yöneticisi'nin kapatma işareti (`StartupApproved\Run`, 03…) yazılınca Ayarlar'da anahtar kapalı göründü ve "Görev Yöneticisi'nde kapatılmış" notu çıktı;
  - Ayarlar'dan yeniden açınca işaret silindi;
  - kapatınca kayıt kalktı.
- **11:** ikinci açılış gizli pencereyi getirdi, tek süreç kaldı.
- **12:** dil İngilizceye geçince tepsi menüsü de geçti (günlük: `tray menu language locale=en`), Türkçeye dönünce geri döndü.
- **13:** 6 dk 20 sn gizli kaldıktan sonra kısılma yok (kullanıcı 10 dakikayı beklemeye gerek görmedi). Ayrıntı "Ölçümler"de.

### Bildirimler (2026-11-12)

**Kimlik:** Windows bildirimi uygulamanın kimliğiyle (AUMID) gösterir.
- Kurulu sürümde kimliği Başlat menüsü kısayolu verir. Geliştirme sürümünde kısayol yok; bu yüzden PowerShell'in kimliği ödünç alınıyordu ve bildirim "Windows PowerShell" adıyla çıkıyordu.
- Artık uygulama açılışta kendi kimliğini (`dev.keymotion`) kaydediyor: `HKCU\Software\Classes\AppUserModelId\dev.keymotion`, `DisplayName` "Keymotion" ve `IconUri` (logo, `%LOCALAPPDATA%\dev.keymotion\notification-icon.png`). Kısayol gerekmiyor.
- Kayıt olmazsa geliştirme sürümü PowerShell'in kimliğine düşer.
- Doğrulama: arka plan bildirimi Windows'un bildirim geçmişinde `dev.keymotion` altında göründü.
- Bildirim başlığında uygulama adı zaten görünüyor. Bu yüzden metin "Arka planda çalışmaya devam ediyor" oldu, ad tekrarlanmıyor.

**Uygulamanın gönderdiği Windows bildirimleri (tam liste):**
1. **Arka planda çalışıyor:** pencere ilk kez tepsiye gittiğinde, bir kez (bilgisayar başına; `background.json` `noticeShown`). Sonraki kapatmalarda yok.
2. **Hatırlatıcı:** bir hatırlatıcı çaldığında başlığı ve notu. Hangi durumda çıkar:
   - "Windows bildirimi de gönder" ayarı açıksa (varsayılan kapalı) her seferinde;
   - klavye USB kablosunda değilse her zaman, çünkü tuşlar yanamaz.
   - Bildirimin kendi sesi yok; hatırlatıcının sesi açıksa Windows sesi ayrıca çalar.
- **Bildirim göndermeyenler:**
  - odak zamanlayıcısı (aşama değişince ses, tuşlar ve ekran kartı);
  - takvim (toplantı öncesi tuş yanıp söner ve ekran kartı çıkar);
  - hatalar ve uyarılar (yalnızca uygulama içinde).
- **Karar (kullanıcı, 2026-11-12): böyle kalsın.** Klavye kablosuzken her hatırlatıcı Windows bildirimi gönderir, çünkü tuşlar yanamaz ve hatırlatıcının ulaşmasının tek yolu bu. Arka plan bildirimi yalnızca ilk seferde çıkar.

**Elle denenecekler** (kullanıcıya verilen senaryolar; simge saatin yanındaki ^ içinde):

1. **Kapatma:** X → pencere kaybolur, simge kalır, "Keymotion arka planda çalışıyor" bildirimi bir kez çıkar.
2. **Sol tık** simgeye → pencere geri gelir.
3. **Sağ tık** → en üstte klavye satırı ("AJAZZ AK980 PRO · USB kablo").
4. **Odak zamanlayıcısı ▸:**
   - boştayken "Hazır: …";
   - Başlat → aşama satırı ve kalan dakika, fare simgenin üstündeyken araç ipucunda da;
   - Duraklat → "Duraklatıldı · …", Sürdür;
   - Sonraki aşamaya geç → mola;
   - Durdur → "Hazır";
   - menü açıkken dakika azalır.
5. **Hatırlatıcılar ▸:** sıradaki hatırlatıcı. "Hatırlatıcılar açık" işareti kaldırılınca "Hatırlatıcılar kapalı" görünür ve sayfadaki anahtar da kapanır. Sonra tekrar aç.
6. **Tuşlarda sistem göstergeleri:** işaretlenince tuşlarda çubuklar çıkar, Sistem göstergeleri sayfasındaki anahtar da açılır. Sonra kapat.
7. **Profiller ▸:** klavyedeki profil işaretli. Başka biri seçilince pencere açılır ve Profiller'de onay satırı görünür; Vazgeç.
8. **Çık:**
   - değişiklik yokken: uygulama kapanır, simge gider;
   - kaydedilmemiş değişiklikle (örneğin tuş başına bir renk): X pencereyi gizler, değişiklik kaybolmaz. Tepsiden Çık → pencere açılır ve "kaydedilmemiş" penceresi çıkar. Vazgeç → uygulama açık kalır, X yine gizler.
9. **Ayar kapalıyken:** Ayarlar › Arka plan › "Kapatınca tepside…" kapalı → X uygulamayı tamamen kapatır. Sonra ayarı geri aç.
10. **Windows'la başlatma:**
    - açınca Görev Yöneticisi › Başlangıç uygulamaları'nda "keymotion" görünür;
    - orada devre dışı bırakılınca Ayarlar'daki anahtar kapalı görünür ve "Görev Yöneticisi'nde kapatılmış" notu çıkar;
    - sonra kapat.
    - Debug sürüm arayüzü Vite'tan yüklediği için gerçek yeniden başlatma kurulu sürümde denenecek.
11. **İkinci açılış:** pencere gizliyken exe'yi yeniden çalıştır → aynı pencere gelir, ikinci simge çıkmaz.
12. **Dil:** Ayarlar'da English → menü İngilizce olur.
13. **Uzun gizli kalma (10 dk ve üstü):** hatırlatıcı ve odak kartları zamanında gelir. Bu, Chromium'un kısma eşiği için.

- kapat → gizlen;
- ilk bildirim;
- tepsi menüsünün her maddesi;
- ikinci açılış;
- Çık: hem kapatma penceresiyle hem pencere olmadan;
- gizliyken kartların ve hatırlatıcının gelmesi;
- Windows'la başlatma: kayıt oluşuyor ve siliniyor, uygulama gizli başlıyor;
- bellek ve güç ölçümleri.

## Aşama 3: Keymotion açılış animasyonu

- **Amaç:** fabrika animasyonunun yedeği olmayanların GIF sayfasına koyabileceği özgün görsel. AJAZZ'ın animasyonu paketlenmez (karar).
- **Konsept:** logodaki üç çizgili "K". Çizgiler sırayla belirir, sağ alttaki turuncu çizgi yanar ve hafifçe parlar. Koyu zemin. 240×135, en çok 140 kare, döngülü.
- **Akış:**
  1. sohbette etkileşimli önizleme ve kullanıcının onayı;
  2. karelerin kodla üretilmesi (tekrarlanabilir);
  3. kütüphaneye yerleşik "Keymotion" öğesi olarak eklenmesi;
  4. fabrika yedeği yoksa "Fabrika animasyonunun yedeği yok" uyarısında öneri olarak sunulması.
- **Ne zaman:** Aşama 2 ile paralel yürüyebilir (ayrı yardımcı). Ekran görüntülerinden (Aşama 7) önce bitmeli.

## Aşama 4: sorun bildirme ve GitHub sorun formları

Kararlar: yalnızca GitHub, e-posta yok. Uygulama kendiliğinden hiçbir şey göndermez. Ayrıntı: `docs/urun-eksikleri.md` bölüm I.

### 4.1 GitHub tarafı (önce)

Uygulama içi düğme bu formların adresini açacak, o yüzden formlar önce yapılır.

- **Sorun formları** `.github/ISSUE_TEMPLATE/`: hata, klavye desteği isteği, öneri. İngilizce, kısa Türkçe açıklamalı.
- **`config.yml`:** soruları Discussions Q&A'ya yönlendirir.
- **`SECURITY.md`:** güvenlik açıkları için GitHub'ın "Private vulnerability reporting" özelliği. Bu bir repo ayarı; açmadan önce kullanıcıya sor.

### 4.2 Uygulama tarafı

- **Beklenmeyen hata:** sakin bir bildirim ve "Sorunu bildir" düğmesi. Aynı hata tekrar gösterilmez. Beklenen durumlar, yani `errors.ts` `EXPECTED`, bildirim istemez.
- **Çökme:** sonraki açılışta bir kez "geçen sefer beklenmedik şekilde kapandı" ve "Bildir". Kaynak `crash-*.txt`.
- **Bildirme penceresi:**
  - gönderilecek metni önce gösterir (tanılama bilgisi, son günlük satırları);
  - "GitHub'da bildir" formu açar (`open_link`, `PROJECT_PAGES` izin listesi) ve ayrıntıyı panoya kopyalar;
  - "Kopyala" ayrıca bulunur.
- **Desteklenmeyen klavye:**
  - Tetikleyici: VID 0C45 (SONiX) ya da AJAZZ ürün adı taşıyan ama listede olmayan bir klavye.
  - "Bu klavye henüz desteklenmiyor — destek iste" düğmesi çıkar.
  - Bildirime VID/PID, ürün adı, arayüzler ve HID tanımlayıcıları eklenir. Yalnızca okunur, klavyeye bir şey yazılmaz.
  - Form: "klavye desteği".

## Aşama 5: kurulum dosyası (NSIS) ve imza

- **Kurulum:**
  - kullanıcı başına kurulum, yönetici izni gerekmez;
  - Başlat menüsü kısayolu; masaüstü kısayolu isteğe bağlı;
  - kurulum dili İngilizce ve Türkçe;
  - WebView2 önyükleyicisi.
- **Kaldırıcı:**
  - "ayarlarımı da sil" seçeneği, varsayılan: sakla;
  - Windows'la başlatma kaydını siler (`Run` ve `StartupApproved\Run` altındaki "Keymotion");
  - bildirim kimliğini siler (`HKCU\Software\Classes\AppUserModelId\dev.keymotion`);
  - çalışan uygulamayı kapatır.
- **Exe bilgileri:** ürün adı, açıklama, sürüm, simge.
- **İmza:** onay gelince sürüm hattına eklenir (Aşama 1b).
- **Doğrulama:**
  - temiz bir Windows kullanıcı hesabında ya da sanal makinede kur, aç, kaldır;
  - SmartScreen davranışı: imzasız ve imzalı;
  - kurulu sürümde sahte arka uç ve geliştirici anahtarları olmamalı (Aşama 0).

## Aşama 6: sürüm hattı ve otomatik güncelleyici

- **GitHub Actions:**
  - `v*` etiketi geldiğinde Windows'ta derler ve imzalar (imza hazırsa);
  - kurulum dosyasını ve `latest.json`'u GitHub Releases'e taslak olarak yükler;
  - sürüm notlarını `CHANGELOG.md`'den alır.
- **Güncelleyici:** `tauri-plugin-updater`.
  - İmza anahtarı `tauri signer generate` ile üretilir. Özel anahtar GitHub Secrets'a konur; bunu kullanıcı onayıyla yap. Ortak anahtar `tauri.conf.json`'a girer.
  - Uygulamada Ayarlar'da "Güncellemeleri denetle" bulunur.
  - Başlangıçta günde bir kez sessiz kontrol yapılır. Ağa yalnızca GitHub için çıkılır.
  - Gizlilik belgeleri güncellenir: uygulama ağa takvimler ve güncelleme denetimi için çıkar.
- **Dallar:** sürüm için `develop` → `main` PR'ı açılır, kullanıcı onayıyla birleşir. Etiket `main`'de atılır.
- **Deneme:** 0.1.0-rc → 0.1.0-rc.2 güncellemesi gerçekten iniyor ve kuruluyor mu?

## Aşama 7: GitHub sayfası ve belgeler (H)

- **Dil düzeni:** `README.md` İngilizce ve varsayılan; `README.tr.md` Türkçe. İkisinin en üstünde "English | Türkçe" bağlantısı. Diğer belgelerde de aynı düzen.
- **İçerik:**
  - ne yapar;
  - desteklenen klavye ve denenen ortam: Windows 11 (26200), klavye yazılımı 1.06, WebView2 sürümü;
  - kurulum ve ilk kullanım;
  - sayfa sayfa kullanım belgeleri;
  - kurtarma rehberi (B9): fabrika animasyonunu geri yükleme, tuş atamalarını sıfırlama;
  - AJAZZ'dan bağımsızlık ve lisans notu;
  - katkı rehberi (`CONTRIBUTING.md`), davranış kuralları;
  - Discussions ve sorun formlarına bağlantılar.
- **Ekran görüntüleri:** TR/EN × açık/koyu. Arayüz kesinleştikten sonra, yani Aşama 2–4'ten sonra çekilir. Sahte cihazla çekilebilir.
- **Destek haritası:** `docs/destek-haritasi.md` İngilizceye çevrilir.

## Aşama 8: son denemeler ve ilk sürüm (0.1.0)

- **Kullanıcının elle denemeleri** (ben yapamıyorum):
  - klavye yokken açılış ekranı (kablo çekilir);
  - yükleme ortasında kablo çekip yeniden açınca "yarım kaldı" önerisi;
  - yükleme sırasında Windows'u kapatmak;
  - küçük ekranda (1920×1080, %150) pencerenin sığması;
  - Bluetooth kanal kısayolları (Fn+Q/W/E) ve denenmemiş tuş atamaları (B8);
  - hatırlatıcı Windows bildiriminin ekranda göründüğü.
- **Kurulu sürümle uçtan uca deneme:** temiz kurulum, güncelleme akışı, kaldırma.
- **Yayın:** `CHANGELOG.md` ve sürüm notları. `main`'e birleştirme kullanıcı onayıyla. Sonra etiket ve yayın.
- **İsteğe bağlı:** sahte cihazla Playwright uçtan uca testlerini CI'a eklemek (DURUM P3).

## Sonraya bırakılanlar (ilk sürümden sonra)

- oyun oturumu, ses spektrumu;
- başka klavye modelleri (P4; `docs/destek-haritasi.md`'deki adaylar, ancak o klavyede denenerek);
- 2.4G alıcısı: sürücü ve pil yüzdesi (`20 01`); Bluetooth işleri;
- uygulamaya göre otomatik profil (ön plandaki pencere), tepsi altyapısının üstüne.
- dışarıdan katkı gelmeye başlayınca denetim düzeni: zorunlu inceleme ve CODEOWNERS, etiketler, fork PR'larında gizli bilgi koruması (DURUM P3).

## Bekleyen kullanıcı kararları (sırası gelince sorulacak)

| Aşama | Soru | Öneri |
|---|---|---|
| 1 | Rust paket ve exe adı `keymotion` olsun mu? | Evet |
| 1b | SignPath Foundation başvurusu | Kullanıcı başvurur, formu birlikte dolduralım |
| 4 | GitHub'da özel güvenlik bildirimi açılsın mı? | Evet |
| 5 | Kurulum ayrıntıları (kısayollar, kaldırırken ayarlar) | Yukarıdaki varsayılanlar |
| 6 | Güncelleme anahtarı GitHub Secrets'a konulsun mu? | Evet |
| — | Disk için `cargo clean` (ortak `target` klasörü) | Gerekirse, sorarak |
